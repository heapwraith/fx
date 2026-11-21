//! fx: the application.
//!
//! `fx [folder]` opens a window listing the folder (default: the current
//! directory). Task T0.5 (docs/phases/PHASE-0.md). `FX_TRACE=1` records timings,
//! see `trace.rs`.
#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod folder_view;
mod trace;

use std::path::PathBuf;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

use folder_view::FolderView;
use trace::Trace;

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    // Window title: folder name, then "fx" (read by the taskbar and the Narrator).
    let title = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let title = format!("{title} — fx");
    let mut trace = Trace::from_env(&path.display().to_string());
    if let Some(t) = &mut trace {
        t.mark("main");
    }

    let app = Application::new();
    if let Some(t) = &mut trace {
        t.mark("app_created");
    }
    app.run(move |cx: &mut App| {
        if let Some(t) = &mut trace {
            t.mark("app_running");
        }
        let bounds = Bounds::centered(None, size(px(900.0), px(640.0)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                if let Some(t) = &mut trace {
                    t.mark("window_created");
                }
                let view = cx.new(|cx| FolderView::new(path, trace, cx));
                // Keys go to the list from the start ("type to filter").
                view.read(cx).focus_handle().focus(window);
                let for_close = view.clone();
                window.on_window_should_close(cx, move |_, cx| {
                    for_close.read(cx).write_trace();
                    true
                });
                view
            },
        );
        if let Err(err) = opened {
            eprintln!("fx: failed to open window: {err:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}

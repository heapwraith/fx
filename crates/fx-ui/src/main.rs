//! fx: the application.
//!
//! `fx [folder]` opens a window listing the folder (default: the current
//! directory). Task T0.5 (docs/phases/PHASE-0.md). `FX_TRACE=1` records timings,
//! see `trace.rs`. Look (task T1.1): follows the Windows light/dark theme;
//! `FX_THEME=light|dark`, `FX_DENSITY=compact|standard|comfortable` and
//! `FX_MICA=0` override it.
#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod folder_view;
mod theme;
mod trace;

use std::path::PathBuf;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowOptions, px, size,
};

use folder_view::{FolderView, Look};
use theme::{Density, Mode, Theme};
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
        let mica_supported =
            fx_shell::supports_mica() && std::env::var_os("FX_MICA").is_none_or(|v| v != "0");
        let font = theme::pick_font(&cx.text_system().all_font_names());
        let forced = Mode::from_env();
        let density = Density::from_env().unwrap_or(Density::Standard);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    ..Default::default()
                }),
                window_background: if mica_supported {
                    WindowBackgroundAppearance::MicaBackdrop
                } else {
                    WindowBackgroundAppearance::Opaque
                },
                ..Default::default()
            },
            move |window, cx| {
                if let Some(t) = &mut trace {
                    t.mark("window_created");
                }
                let system = Mode::from_appearance(window.appearance());
                let mode = forced.unwrap_or(system);
                let mica = mica_supported && mode == system;
                if mica_supported && !mica {
                    window.set_background_appearance(WindowBackgroundAppearance::Opaque);
                }
                let look = Look {
                    theme: Theme::for_mode(mode),
                    forced_mode: forced.is_some(),
                    density,
                    font,
                    mica_supported,
                    mica,
                };
                let view = cx.new(|cx| FolderView::new(path, look, trace, cx));
                // Follow the Windows light/dark setting live.
                let themed = view.clone();
                window
                    .observe_window_appearance(move |window, cx| {
                        let system = Mode::from_appearance(window.appearance());
                        let mica = themed.update(cx, |view, cx| {
                            cx.notify();
                            view.set_system_mode(system)
                        });
                        if mica_supported {
                            window.set_background_appearance(if mica {
                                WindowBackgroundAppearance::MicaBackdrop
                            } else {
                                WindowBackgroundAppearance::Opaque
                            });
                        }
                    })
                    .detach();
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

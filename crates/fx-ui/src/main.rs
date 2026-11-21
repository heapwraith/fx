//! fx: the application.
//!
//! `fx [folder]` opens a window listing the folder (default: the current
//! directory). Task T0.5 (docs/phases/PHASE-0.md). `FX_TRACE=1` records timings,
//! see `trace.rs`. Look (task T1.1): follows the Windows light/dark theme;
//! `FX_THEME=light|dark`, `FX_DENSITY=compact|standard|comfortable` and
//! `FX_MICA=0` override it.
//!
//! Single instance (D-013, task T1.2): if fx is already running in this
//! session, the folder is handed to it (it opens a new window) and this process
//! exits. `FX_TRACE=1` (benchmarks) and `FX_SINGLE_INSTANCE=0` skip this, so
//! every process measures itself.
#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod folder_view;
mod theme;
mod trace;

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowOptions, px, size,
};

use folder_view::{FolderView, Look, window_title};
use theme::{Density, Mode, Theme};
use trace::Trace;

/// Settings read once at startup and shared by every window.
#[derive(Debug, Clone, Copy)]
struct Startup {
    mica_supported: bool,
    font: &'static str,
    forced: Option<Mode>,
    density: Density,
}

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    // Absolute, so "go up" works from a relative argument such as ".".
    let path = std::path::absolute(&path).unwrap_or(path);
    let mut trace = Trace::from_env(&path.display().to_string());
    if let Some(t) = &mut trace {
        t.mark("main");
    }
    let single = trace.is_none() && std::env::var_os("FX_SINGLE_INSTANCE").is_none_or(|v| v != "0");
    let incoming = if single {
        match fx_shell::claim_instance(&path) {
            Ok(fx_shell::Instance::Forwarded) => return,
            Ok(fx_shell::Instance::Primary(rx)) => Some(rx),
            // Run on our own if the pipe misbehaves.
            Err(_) => None,
        }
    } else {
        None
    };

    let app = Application::new();
    if let Some(t) = &mut trace {
        t.mark("app_created");
    }
    app.run(move |cx: &mut App| {
        if let Some(t) = &mut trace {
            t.mark("app_running");
        }
        let startup = Startup {
            mica_supported: fx_shell::supports_mica()
                && std::env::var_os("FX_MICA").is_none_or(|v| v != "0"),
            font: theme::pick_font(&cx.text_system().all_font_names()),
            forced: Mode::from_env(),
            density: Density::from_env().unwrap_or(Density::Standard),
        };
        if !open_folder_window(path, startup, trace, cx) {
            cx.quit();
            return;
        }
        if let Some(rx) = incoming {
            listen(rx, startup, cx);
        }
        cx.activate(true);
    });
}

/// Opens a window for every folder another fx process hands over.
fn listen(rx: Receiver<PathBuf>, startup: Startup, cx: &mut App) {
    cx.spawn(async move |cx| {
        let background = cx.background_executor().clone();
        let mut rx = rx;
        loop {
            // Wait on a background thread, never on the UI thread.
            let (back, folder) = background
                .spawn(async move {
                    let folder = rx.recv();
                    (rx, folder)
                })
                .await;
            rx = back;
            let Ok(folder) = folder else { break };
            if cx
                .update(|cx| open_folder_window(folder, startup, None, cx))
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

/// Opens a folder window; returns false if the window could not be created.
fn open_folder_window(
    path: PathBuf,
    startup: Startup,
    mut trace: Option<Trace>,
    cx: &mut App,
) -> bool {
    let bounds = Bounds::centered(None, size(px(900.0), px(640.0)), cx);
    let Startup {
        mica_supported,
        font,
        forced,
        density,
    } = startup;
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(window_title(&path).into()),
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
            window.activate_window();
            let for_close = view.clone();
            window.on_window_should_close(cx, move |_, cx| {
                for_close.read(cx).write_trace();
                true
            });
            view
        },
    );
    match opened {
        Ok(_) => true,
        Err(err) => {
            eprintln!("fx: failed to open window: {err:#}");
            false
        }
    }
}

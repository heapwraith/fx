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

mod a11y;
/// `0.1.0+d49b858` (`-dirty` when built with local changes): the version
/// and the commit it was built from (crates/fx-ui/build.rs).
pub(crate) const VERSION: &str = env!("FX_VERSION");

mod commands;
mod crash;
mod folder_view;
mod i18n;
mod icons;
mod ops;
mod persist;
mod places;
mod store;
mod tabs_view;
mod tags;
mod theme;
mod thumbs;
mod trace;
mod update;

#[allow(unused_imports)]
use fx_core::{tr, trf};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Receiver;

use gpui::{
    App, AppContext, Application, Bounds, Entity, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowOptions, point, px, size,
};

use a11y::A11yHandle;
use folder_view::{Look, window_title};
use icons::{Icons, Media};
use ops::Ops;
use persist::Persist;
use places::Places;
use store::{Settings, WindowState};
use tabs_view::{TabsView, WindowServices};
use theme::{Density, Mode, Theme};
use thumbs::Thumbs;
use trace::Trace;

/// App-wide state shared by every window.
#[derive(Clone)]
struct Shared {
    places: Entity<Places>,
    media: Media,
    persist: Entity<Persist>,
}

/// Settings read once at startup and shared by every window.
#[derive(Debug, Clone, Copy)]
struct Startup {
    mica_supported: bool,
    font: &'static str,
    icon_font: &'static str,
    forced: Option<Mode>,
    density: Density,
    sidebar: bool,
    show_hidden: bool,
}

/// The daily update check runs once per process.
static UPDATE_CHECKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn main() {
    crash::install();
    tags::init();
    // An empty argument counts as none (a shell command with "%1" and no item).
    let arg = std::env::args_os().nth(1).filter(|a| !a.is_empty());
    // No folder (Win+E, the Start Menu): the user's home, not the working
    // directory, which for those launches is System32.
    let path = arg
        .clone()
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
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

    // Before the event loop: RtlGetVersion only, no I/O.
    let mica_supported =
        fx_shell::supports_mica() && std::env::var_os("FX_MICA").is_none_or(|v| v != "0");
    // Settings and the saved session (task T1.17), read before the event
    // loop. Benchmarks and test runs (FX_NO_STATE=1) neither read nor save.
    let persist = trace.is_none() && std::env::var_os("FX_NO_STATE").is_none_or(|v| v != "1");
    let settings = if persist {
        store::settings_path()
            .map(|p| store::load_settings(&p))
            .unwrap_or_default()
    } else {
        Settings::default()
    };
    i18n::init(&settings.idioma);
    crash::set_enabled(settings.relatorio_falhas);
    // Benchmarks and test runs never ask the network.
    update::set_enabled(persist && settings.verificar_atualizacoes);
    // A report left by the last run (T3.10): offered in the status bar.
    let crashed = (persist && settings.relatorio_falhas)
        .then(crash::pending)
        .flatten()
        .is_some();
    crash::set_notice(crashed);
    // Test hook for the crash report (T3.10).
    if std::env::var_os("FX_TEST_PANIC").is_some_and(|v| v == "1") {
        panic!("crash report test at C:\\Users\\example\\x.txt");
    }
    let session = (persist && arg.is_none() && settings.restaurar_sessao)
        .then(persist::load_session)
        .flatten();

    let app = Application::new();
    if let Some(t) = &mut trace {
        t.mark("app_created");
    }
    app.run(move |cx: &mut App| {
        if let Some(t) = &mut trace {
            t.mark("app_running");
        }
        let fonts = cx.text_system().all_font_names();
        let startup = Startup {
            mica_supported,
            font: theme::pick_font(&fonts),
            icon_font: theme::pick_icon_font(&fonts),
            forced: Mode::from_env(),
            density: Density::from_env().unwrap_or(match settings.densidade.as_str() {
                "compacta" => Density::Compact,
                "confortável" => Density::Comfortable,
                _ => Density::Standard,
            }),
            sidebar: settings.barra_lateral,
            show_hidden: settings.mostrar_ocultos,
        };
        let trace = trace.map(|t| Rc::new(RefCell::new(t)));
        let pinned = session.as_ref().and_then(|s| s.pinned.clone());
        let places = cx.new(|cx| Places::new(trace.clone(), pinned, cx));
        let media = Media {
            icons: cx.new(Icons::new),
            thumbs: cx.new(Thumbs::new),
            ops: cx.new(Ops::new),
        };
        let persist = cx.new(|cx| Persist::new(persist, settings, places.clone(), cx));
        let shared = Shared {
            places,
            media,
            persist,
        };
        // The last session's windows, or one window on the folder asked for.
        let saved: Vec<WindowState> = session
            .map(|s| s.windows)
            .unwrap_or_default()
            .into_iter()
            .filter(|w| !w.tabs.is_empty())
            .collect();
        let mut opened = 0;
        if saved.is_empty() {
            opened += usize::from(open_folder_window(
                path,
                startup,
                trace,
                shared.clone(),
                None,
                cx,
            ));
        } else {
            for w in saved {
                let first = w.tabs[0].path.clone();
                opened += usize::from(open_folder_window(
                    first,
                    startup,
                    None,
                    shared.clone(),
                    Some(w),
                    cx,
                ));
            }
        }
        if opened == 0 {
            cx.quit();
            return;
        }
        bench_copy(&shared, cx);
        if let Some(rx) = incoming {
            listen(rx, startup, shared, cx);
        }
        cx.activate(true);
    });
}

/// Benchmark hook (task T1.13): `FX_BENCH_COPY=<source>|<folder>` starts a
/// copy at launch, so the list can be measured while it runs.
fn bench_copy(shared: &Shared, cx: &mut App) {
    let Ok(spec) = std::env::var("FX_BENCH_COPY") else {
        return;
    };
    let Some((src, dest)) = spec.split_once('|') else {
        return;
    };
    let job = fx_ops::Job::copy(vec![PathBuf::from(src)], PathBuf::from(dest))
        .on_conflict(fx_ops::Resolution::Ask);
    let _ = shared.media.ops.update(cx, |ops, _| ops.submit(job));
}

/// Opens a window for every folder another fx process hands over.
fn listen(rx: Receiver<PathBuf>, startup: Startup, shared: Shared, cx: &mut App) {
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
                .update(|cx| open_folder_window(folder, startup, None, shared.clone(), None, cx))
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

fn track(persist: &Entity<Persist>, handle: WindowHandle<TabsView>, cx: &mut App) {
    persist.update(cx, |p, _| p.track(handle));
}

/// Opens a folder window; returns false if the window could not be created.
fn open_folder_window(
    path: PathBuf,
    startup: Startup,
    trace: Option<Rc<RefCell<Trace>>>,
    shared: Shared,
    saved: Option<WindowState>,
    cx: &mut App,
) -> bool {
    let bounds = match saved.as_ref().and_then(|s| s.bounds) {
        Some([x, y, w, h]) if w >= 200.0 && h >= 150.0 => Bounds {
            origin: point(px(x), px(y)),
            size: size(px(w), px(h)),
        },
        _ => Bounds::centered(None, size(px(900.0), px(640.0)), cx),
    };
    let Startup {
        mica_supported,
        font,
        icon_font,
        forced,
        density,
        sidebar,
        show_hidden,
    } = startup;
    let persist = shared.persist.clone();
    let close_persist = persist.clone();
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(window_title(&path).into()),
                ..Default::default()
            }),
            // Hidden until the accessibility adapter is attached (D-008): it
            // must subclass the window before it is shown.
            show: false,
            window_background: if mica_supported {
                WindowBackgroundAppearance::MicaBackdrop
            } else {
                WindowBackgroundAppearance::Opaque
            },
            ..Default::default()
        },
        move |window, cx| {
            if let Some(t) = &trace {
                t.borrow_mut().mark("window_created");
            }
            let system = Mode::from_appearance(window.appearance());
            let mode = forced.unwrap_or(system);
            let theme = Theme::current(mode);
            let mica = mica_supported && mode == system && !theme.high_contrast;
            if mica_supported && !mica {
                window.set_background_appearance(WindowBackgroundAppearance::Opaque);
            }
            let look = Look {
                theme,
                forced_mode: forced.is_some(),
                density,
                font,
                icon_font,
                grid: std::env::var_os("FX_VIEW").is_some_and(|v| v == "grid"),
                sidebar,
                show_hidden,
                mica_supported,
                mica,
            };
            let latest: fx_shell::LatestTree = Rc::new(RefCell::new(None));
            let a11y = fx_shell::A11y::attach(window, latest.clone())
                .map(|(adapter, actions)| (A11yHandle::new(adapter, latest), actions));
            let view = cx.new(|cx| {
                let services = WindowServices {
                    places: shared.places,
                    media: shared.media,
                    a11y,
                };
                TabsView::new(path, look, trace, services, window, cx)
            });
            if let Some(state) = saved {
                view.update(cx, |view, cx| view.restore_tabs(state, window, cx));
            }
            // Daily update check (T4.1), first window only, off the UI thread.
            if update::enabled() && !UPDATE_CHECKED.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                let tab = view.read(cx).current();
                tab.update(cx, |tab, cx| tab.check_updates(true, cx));
            }
            if crash::take_notice() {
                let tab = view.read(cx).current();
                tab.update(cx, |tab, cx| {
                    tab.show_notice(
                        tr!(
                            "Falha na última abertura · Ctrl+K: relatar",
                            "Crashed last time · Ctrl+K: report"
                        )
                        .to_owned(),
                        cx,
                    );
                });
            }
            // Follow the Windows light/dark setting live.
            let themed = view.clone();
            window
                .observe_window_appearance(move |window, cx| {
                    let system = Mode::from_appearance(window.appearance());
                    let mica = themed.update(cx, |view, cx| view.set_system_mode(system, cx));
                    if mica_supported {
                        window.set_background_appearance(if mica {
                            WindowBackgroundAppearance::MicaBackdrop
                        } else {
                            WindowBackgroundAppearance::Opaque
                        });
                    }
                })
                .detach();
            // High contrast does not change the light/dark appearance GPUI
            // reports, so its switch is polled (one cheap system call a second).
            let contrast_view = view.clone();
            let handle = window.window_handle();
            cx.spawn(async move |cx| {
                let mut on = fx_shell::high_contrast().is_some();
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(1))
                        .await;
                    let now = fx_shell::high_contrast().is_some();
                    if now == on {
                        continue;
                    }
                    on = now;
                    let updated = handle.update(cx, |_, window, cx| {
                        let system = Mode::from_appearance(window.appearance());
                        let mica =
                            contrast_view.update(cx, |view, cx| view.set_system_mode(system, cx));
                        if mica_supported {
                            window.set_background_appearance(if mica {
                                WindowBackgroundAppearance::MicaBackdrop
                            } else {
                                WindowBackgroundAppearance::Opaque
                            });
                        }
                    });
                    if updated.is_err() {
                        break;
                    }
                }
            })
            .detach();
            window.activate_window();
            let for_close = view.clone();
            let closing_persist = close_persist;
            window.on_window_should_close(cx, move |window, cx| {
                let tabs = for_close.read(cx);
                tabs.write_trace();
                let state = tabs.export(window, cx);
                let settings = tabs.settings_part();
                let id = window.window_handle().window_id();
                closing_persist.update(cx, |p, cx| p.closing(id, state, settings, cx));
                true
            });
            view
        },
    );
    match opened {
        Ok(handle) => {
            track(&persist, handle, cx);
            true
        }
        Err(err) => {
            eprintln!("fx: failed to open window: {err:#}");
            false
        }
    }
}

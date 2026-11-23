//! fx-setup (task T4.3): fx's own installer, with fx's look (GPUI, Fluent
//! tokens from `fx-ui/src/theme.rs`, light/dark, Portuguese/English).
//!
//! ```text
//! fx-setup                      install (the setup carries the package)
//! fx-setup --silent [--wait <pid>] [--relaunch]
//!                               install without a window (exit code); the
//!                               update helper waits for fx to close first
//! fx-setup --uninstall [--quiet]
//! fx-setup --pack <fx-setup-boot.exe> <bin dir> <doc dir> <out>   (CI)
//!
//! The downloaded installer is `fx-setup-boot` with the package (T5.7): it
//! runs this program out of the package with `--package <installer>`.
//! ```
//!
//! The installed copy (`%LOCALAPPDATA%\Programs\fx\fx-setup.exe`, no
//! package) is the uninstaller registered in Settings › Apps.
#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use std::time::Duration;

use fx_core::{tr, trf};
use fx_setup::pack::{self, Package};
use fx_setup::transaction::{self, Options, Step, Target};
use gpui::{
    App, Application, Bounds, Context, FocusHandle, Image, ImageFormat, KeyDownEvent, MouseButton,
    SharedString, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions,
    div, img, prelude::*, px, size,
};

use fx_theme as theme;

use theme::{BODY_SIZE, CAPTION_SIZE, Mode, RADIUS, Theme, rgba, space};

const VERSION: &str = env!("FX_VERSION");
const LOGO: &[u8] = include_bytes!("../../fx-ui/resources/logo-128.png");
const ICON_CHECK: &str = "\u{EC61}";
const ICON_WARNING: &str = "\u{E7BA}";
const ICON_CHECKBOX: &str = "\u{E73A}";
const ICON_CHECKBOX_EMPTY: &str = "\u{E739}";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lang = std::env::var("FX_LANG").ok();
    fx_core::locale::set_english(match lang.as_deref() {
        Some("en") => true,
        Some("pt") => false,
        _ => !fx_shell::system_language_is_portuguese(),
    });
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value_of = |f: &str| {
        args.iter()
            .position(|a| a == f)
            .and_then(|i| args.get(i + 1))
    };
    if let [cmd, stub, bins, docs, out, rest @ ..] = args.as_slice()
        && cmd == "--pack"
    {
        // `--version <v>`: a different version, for the update tests (T4.4).
        let version = match rest {
            [] => transaction::display_version(VERSION),
            [opt, v] if opt == "--version" => v.clone(),
            _ => {
                eprintln!("fx-setup --pack <programa> <binários> <raiz> <saída> [--version <v>]");
                return ExitCode::FAILURE;
            }
        };
        return match pack::pack_dirs(
            Path::new(stub),
            Path::new(bins),
            Path::new(docs),
            &version,
            Path::new(out),
        ) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("fx-setup --pack: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let Some(target) = Target::user(value_of("--test-root").map(Path::new)) else {
        return ExitCode::FAILURE;
    };
    remove_stale_copies();
    if flag("--uninstall") {
        // The installed copy cannot delete itself: run from a copy in %TEMP%.
        if let Ok(exe) = std::env::current_exe()
            && exe.starts_with(&target.dir)
            && let Some(copy) = copy_to_temp(&exe)
        {
            let started = std::process::Command::new(copy).args(&args).spawn();
            return if started.is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        let code = if flag("--quiet") {
            match transaction::uninstall(&target, &[], &mut |_| {}) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::FAILURE,
            }
        } else {
            run_ui(Screen::Confirm { remove_data: false }, None, target);
            ExitCode::SUCCESS
        };
        // This copy in %TEMP% goes once it has ended (T5.6).
        if let Some(me) = running_copy() {
            let _ = fx_shell::install::delete_after_exit(&me);
        }
        return code;
    }
    // Started by the launcher (T5.7): the package is in the downloaded file.
    let carried = match value_of("--package") {
        Some(file) => std::fs::read(file).and_then(|bytes| pack::read(&bytes)),
        None => pack::read_self(),
    };
    let package = match carried {
        Ok(Some(p)) => Arc::new(p),
        // The installed copy, opened without arguments: offer to uninstall.
        Ok(None) => {
            run_ui(Screen::Confirm { remove_data: false }, None, target);
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            run_ui(Screen::Failed(damaged(&e)), None, target);
            return ExitCode::FAILURE;
        }
    };
    if flag("--silent") {
        // The update helper (T4.4): wait for fx to close, replace the files,
        // and open the new fx again if this was a restart.
        let wait = value_of("--wait").and_then(|p| p.parse::<u32>().ok());
        if let Some(pid) = wait {
            fx_shell::install::wait_for_exit(pid, Duration::from_secs(60));
        }
        let opts = Options {
            desktop_shortcut: false,
        };
        let result = transaction::install(&package, &target, opts, &mut |_| {});
        if flag("--relaunch") {
            let _ = std::process::Command::new(target.exe()).spawn();
        }
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };
    }
    run_ui(Screen::Welcome { options: false }, Some(package), target);
    ExitCode::SUCCESS
}

fn damaged(e: &std::io::Error) -> String {
    trf!(
        "Este instalador está danificado ({e}). Baixe-o de novo.",
        "This installer is damaged ({e}). Download it again."
    )
}

/// The name of the uninstaller's copies in %TEMP%.
const COPY_PREFIX: &str = "fx-uninstall-";

/// This program, when it is a copy of the uninstaller in %TEMP%.
fn running_copy() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let name = me.file_name()?.to_str()?;
    (name.starts_with(COPY_PREFIX)
        && name.ends_with(".exe")
        && me.parent() == Some(std::env::temp_dir().as_path()))
    .then_some(me)
}

/// Copies left by earlier uninstalls (killed before cleaning up): removed;
/// one still running is locked and stays.
fn remove_stale_copies() {
    let me = std::env::current_exe().ok();
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let ours = e
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(COPY_PREFIX) && n.ends_with(".exe"));
        if ours && me.as_deref() != Some(path.as_path()) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn copy_to_temp(exe: &Path) -> Option<PathBuf> {
    let bytes = std::fs::read(exe).ok()?;
    let copy = std::env::temp_dir().join(format!("{COPY_PREFIX}{}.exe", std::process::id()));
    std::fs::write(&copy, pack::program_of(&bytes)).ok()?;
    Some(copy)
}

#[derive(Debug, Clone, PartialEq)]
enum Screen {
    Welcome { options: bool },
    Working { uninstall: bool },
    Done,
    Failed(String),
    Confirm { remove_data: bool },
    Removed,
}

struct Setup {
    screen: Screen,
    package: Option<Arc<Package>>,
    target: Target,
    installed: Option<String>,
    desktop_shortcut: bool,
    /// Progress shown (eases towards `goal`), 0..1.
    shown: f32,
    goal: f32,
    /// Files of the current install or uninstall (from the real progress).
    files: usize,
    step: String,
    theme: Theme,
    mica: bool,
    font: &'static str,
    icon_font: &'static str,
    focus: FocusHandle,
    logo: Arc<Image>,
    /// The fx-indexd service installed for the whole machine (T2.5).
    service: bool,
}

fn run_ui(screen: Screen, package: Option<Arc<Package>>, target: Target) {
    Application::new().run(move |cx: &mut App| {
        let fonts = cx.text_system().all_font_names();
        let (font, icon_font) = (theme::pick_font(&fonts), theme::pick_icon_font(&fonts));
        let mica_supported = fx_shell::supports_mica();
        let title = if matches!(screen, Screen::Confirm { .. }) {
            tr!("Desinstalar o Pathdash", "Uninstall Pathdash")
        } else {
            tr!("Instalar o Pathdash", "Install Pathdash")
        };
        let bounds = Bounds::centered(None, size(px(640.0), px(440.0)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    ..Default::default()
                }),
                is_resizable: false,
                is_minimizable: true,
                window_background: if mica_supported {
                    WindowBackgroundAppearance::MicaBackdrop
                } else {
                    WindowBackgroundAppearance::Opaque
                },
                ..Default::default()
            },
            move |window, cx| {
                let system = Mode::from_appearance(window.appearance());
                let theme = Theme::current(system);
                let mica = mica_supported && !theme.high_contrast;
                if mica_supported && !mica {
                    window.set_background_appearance(WindowBackgroundAppearance::Opaque);
                }
                let installed = transaction::installed_version(&target);
                let service = std::env::var_os("ProgramFiles")
                    .is_some_and(|p| Path::new(&p).join("fx").join("fx-indexd.exe").exists());
                let view = cx.new(|cx| Setup {
                    screen,
                    package,
                    target,
                    installed,
                    desktop_shortcut: false,
                    shown: 0.0,
                    goal: 0.0,
                    files: 0,
                    step: String::new(),
                    theme,
                    mica,
                    font,
                    icon_font,
                    focus: cx.focus_handle(),
                    logo: Arc::new(Image::from_bytes(ImageFormat::Png, LOGO.to_vec())),
                    service,
                });
                view.update(cx, |v, _| v.focus.focus(window));
                let themed = view.clone();
                window
                    .observe_window_appearance(move |window, cx| {
                        let mode = Mode::from_appearance(window.appearance());
                        themed.update(cx, |v, cx| {
                            v.theme = Theme::current(mode);
                            cx.notify();
                        });
                    })
                    .detach();
                view
            },
        );
        if opened.is_ok() {
            cx.activate(true);
        } else {
            cx.quit();
        }
    });
}

impl Setup {
    fn primary(&mut self, cx: &mut Context<Self>) {
        match self.screen.clone() {
            Screen::Welcome { .. } => self.start_install(cx),
            Screen::Failed(_) if self.package.is_some() => self.start_install(cx),
            Screen::Done => {
                let _ = std::process::Command::new(self.target.exe()).spawn();
                cx.quit();
            }
            Screen::Confirm { remove_data } => self.start_uninstall(remove_data, cx),
            Screen::Working { .. } => {}
            Screen::Failed(_) | Screen::Removed => cx.quit(),
        }
    }

    fn secondary(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.screen, Screen::Working { .. }) {
            cx.quit();
        }
    }

    fn start_install(&mut self, cx: &mut Context<Self>) {
        let Some(pkg) = self.package.clone() else {
            return;
        };
        let target = self.target.clone();
        let opts = Options {
            desktop_shortcut: self.desktop_shortcut,
        };
        let (tx, rx) = unbounded::<Result<Step, String>>();
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let result = transaction::install(&pkg, &target, opts, &mut |s| {
                let _ = progress_tx.unbounded_send(Ok(s));
            });
            if let Err(e) = result {
                let _ = tx.unbounded_send(Err(install_failure_text(&e)));
            }
        });
        self.begin(false, rx, cx);
    }

    fn start_uninstall(&mut self, remove_data: bool, cx: &mut Context<Self>) {
        let target = self.target.clone();
        let (tx, rx) = unbounded::<Result<Step, String>>();
        std::thread::spawn(move || {
            let data = if remove_data {
                transaction::data_dirs()
            } else {
                Vec::new()
            };
            let progress_tx = tx.clone();
            let _ = tx.unbounded_send(
                match transaction::uninstall(&target, &data, &mut |s| {
                    let _ = progress_tx.unbounded_send(Ok(s));
                }) {
                    Ok(()) => Ok(Step::Done),
                    Err(e) => Err(trf!(
                        "{e}. Feche todas as janelas do Pathdash e tente de novo.",
                        "{e}. Close every Pathdash window and try again."
                    )),
                },
            );
        });
        self.begin(true, rx, cx);
    }

    /// Follows the worker's progress; the bar eases every frame.
    fn begin(
        &mut self,
        uninstall: bool,
        rx: UnboundedReceiver<Result<Step, String>>,
        cx: &mut Context<Self>,
    ) {
        self.screen = Screen::Working { uninstall };
        self.shown = 0.0;
        self.goal = 0.0;
        self.step = if uninstall {
            tr!("Removendo o Pathdash…", "Removing Pathdash…")
        } else {
            tr!("Preparando…", "Preparing…")
        }
        .to_owned();
        // The next render keeps requesting frames while the bar moves
        // (request_animation_frame only works while drawing).
        cx.notify();
        cx.spawn(async move |this, cx| {
            let mut rx = rx;
            loop {
                // A message, or a 50 ms tick that keeps the bar moving; no
                // thread waits for either (review, D-066).
                let tick = cx.background_executor().timer(Duration::from_millis(50));
                let message = match futures::future::select(rx.next(), tick).await {
                    futures::future::Either::Left((Some(m), _)) => Some(m),
                    futures::future::Either::Left((None, _)) => break,
                    futures::future::Either::Right(_) => None,
                };
                let finished = this
                    .update(cx, |v, cx| {
                        let end = match message {
                            Some(Ok(step)) => v.advance(step, uninstall),
                            Some(Err(text)) => {
                                v.screen = Screen::Failed(text);
                                true
                            }
                            None => false,
                        };
                        cx.notify();
                        end
                    })
                    .unwrap_or(true);
                if finished {
                    break;
                }
            }
        })
        .detach();
    }

    /// Returns whether the work is over.
    fn advance(&mut self, step: Step, uninstall: bool) -> bool {
        match step {
            // Fractions of real work: each file is one unit, plus one for the
            // shortcuts and one for the registry entry (uninstall: the entry).
            Step::Copying(done, total) => {
                self.files = total;
                self.goal = goal_for(step, total, uninstall);
                if uninstall {
                    self.step = trf!(
                        "Removendo arquivos ({done} de {total})…",
                        "Removing files ({done} of {total})…"
                    );
                } else {
                    self.step = trf!(
                        "Copiando arquivos ({done} de {total})…",
                        "Copying files ({done} of {total})…"
                    );
                }
                false
            }
            Step::Shortcuts => {
                self.goal = goal_for(step, self.files, uninstall);
                self.step = tr!("Criando atalhos…", "Creating shortcuts…").to_owned();
                false
            }
            Step::Registering => {
                self.goal = goal_for(step, self.files, uninstall);
                self.step = if uninstall {
                    tr!(
                        "Removendo dos Aplicativos do Windows…",
                        "Removing from Windows Apps…"
                    )
                } else {
                    tr!(
                        "Registrando nos Aplicativos do Windows…",
                        "Registering in Windows Apps…"
                    )
                }
                .to_owned();
                false
            }
            Step::Done => {
                self.goal = 1.0;
                self.shown = 1.0;
                self.screen = if uninstall {
                    Screen::Removed
                } else {
                    Screen::Done
                };
                true
            }
        }
    }

    fn on_key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match e.keystroke.key.as_str() {
            "enter" => self.primary(cx),
            "escape" => self.secondary(cx),
            _ => return,
        }
        cx.stop_propagation();
    }
}

/// How much of the real work is done when `step` arrives: each file is one
/// unit, plus one for the shortcuts and one for the Apps entry (uninstall:
/// the entry only). Nothing is estimated (status is always real).
fn goal_for(step: Step, files: usize, uninstall: bool) -> f32 {
    let units = (files + if uninstall { 1 } else { 2 }).max(1) as f32;
    let done = match step {
        Step::Copying(done, _) => done,
        // Every file is in place.
        Step::Shortcuts => files,
        Step::Registering if uninstall => files,
        Step::Registering => files + 1,
        Step::Done => return 1.0,
    };
    (done as f32 / units).min(1.0)
}

/// What the failure screen says: exactly what was left on disk.
fn install_failure_text(e: &transaction::InstallError) -> String {
    use transaction::{InstallError, Missing};
    match e {
        InstallError::NothingChanged(error) => trf!(
            "{error}. Nenhum arquivo foi alterado. Tente de novo; se o erro voltar, baixe o instalador outra vez.",
            "{error}. No files were changed. Try again; if the error comes back, download the installer again."
        ),
        InstallError::Incomplete {
            missing: Missing::Shortcut,
            error,
        } => trf!(
            "{error}. Os arquivos novos do Pathdash já estão instalados, mas faltaram o atalho e o registro em Configurações › Aplicativos. Tente de novo para completar.",
            "{error}. Pathdash's new files are installed, but the shortcut and the entry in Settings › Apps are missing. Try again to finish."
        ),
        InstallError::Incomplete {
            missing: Missing::AppsEntry,
            error,
        } => trf!(
            "{error}. Os arquivos novos e o atalho do Pathdash já estão instalados, mas faltou o registro em Configurações › Aplicativos. Tente de novo para completar.",
            "{error}. Pathdash's new files and shortcut are installed, but the entry in Settings › Apps is missing. Try again to finish."
        ),
    }
}

impl Render for Setup {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        // The bar shows exactly the work done (no animation between steps).
        self.shown = self.goal;
        let new = transaction::display_version(VERSION);
        let slogan = tr!(
            "Qualquer pasta, numa arrancada.",
            "Every path, one dash away."
        );
        let failed_title = if self.package.is_some() {
            tr!("A instalação não terminou", "Installation didn't finish")
        } else {
            tr!("A desinstalação não terminou", "Uninstall didn't finish")
        };
        let (title, subtitle, body): (String, Option<&str>, String) = match &self.screen {
            Screen::Welcome { .. } => match &self.installed {
                Some(old) if *old == new => (
                    tr!("Reinstalar o Pathdash", "Reinstall Pathdash").into(),
                    Some(slogan),
                    trf!(
                        "A versão {new} já está instalada. Reinstalar troca só os arquivos do programa; suas configurações ficam.",
                        "Version {new} is already installed. Reinstalling only replaces the program files; your settings stay."
                    ),
                ),
                Some(old) => (
                    tr!("Atualizar o Pathdash", "Update Pathdash").into(),
                    Some(slogan),
                    trf!(
                        "Da versão {old} para a {new}. Suas abas, pastas fixadas e configurações ficam como estão.",
                        "From version {old} to {new}. Your tabs, pinned folders and settings stay as they are."
                    ),
                ),
                None => (
                    tr!("Bem-vindo ao Pathdash", "Welcome to Pathdash").into(),
                    Some(slogan),
                    tr!(
                        "Leva poucos segundos. Instala só para você, sem pedir senha de administrador.",
                        "Takes a few seconds. Installs just for you, no administrator password needed."
                    )
                    .into(),
                ),
            },
            Screen::Working { uninstall } => (
                if *uninstall {
                    tr!("Removendo o Pathdash", "Removing Pathdash")
                } else {
                    tr!("Instalando o Pathdash", "Installing Pathdash")
                }
                .into(),
                Some(slogan),
                self.step.clone(),
            ),
            Screen::Done => (
                tr!("Tudo pronto", "You're all set").into(),
                Some(tr!("O Pathdash já está no Menu Iniciar.", "Pathdash is now in the Start menu.")),
                tr!(
                    "Dica: Ctrl+K abre a paleta com todos os comandos, e F1 mostra como usar o Pathdash.",
                    "Tip: Ctrl+K opens the palette with every command, and F1 shows how to use Pathdash."
                )
                .into(),
            ),
            Screen::Failed(e) => (failed_title.into(), None, e.clone()),
            Screen::Confirm { .. } => (
                tr!("Desinstalar o Pathdash?", "Uninstall Pathdash?").into(),
                None,
                tr!(
                    "O programa e os atalhos saem deste computador. Suas configurações e abas salvas ficam, a menos que você marque a opção abaixo.",
                    "The program and its shortcuts leave this computer. Your settings and saved tabs stay unless you check the option below."
                )
                .into(),
            ),
            Screen::Removed => (
                tr!("O Pathdash foi removido", "Pathdash was removed").into(),
                None,
                tr!(
                    "Obrigado por usar o Pathdash. Se algo fez falta, conte para nós em github.com/heapwraith/fx.",
                    "Thanks for using Pathdash. If something was missing, tell us at github.com/heapwraith/fx."
                )
                .into(),
            ),
        };
        let (primary, secondary): (Option<&str>, Option<&str>) = match &self.screen {
            Screen::Welcome { .. } => (
                Some(match &self.installed {
                    Some(old) if *old == new => tr!("Reinstalar", "Reinstall"),
                    Some(_) => tr!("Atualizar", "Update"),
                    None => tr!("Instalar", "Install"),
                }),
                Some(tr!("Cancelar", "Cancel")),
            ),
            Screen::Working { .. } => (None, None),
            Screen::Done => (
                Some(tr!("Abrir o Pathdash", "Open Pathdash")),
                Some(tr!("Fechar", "Close")),
            ),
            Screen::Failed(_) if self.package.is_some() => (
                Some(tr!("Tentar de novo", "Try again")),
                Some(tr!("Fechar", "Close")),
            ),
            Screen::Failed(_) | Screen::Removed => (Some(tr!("Fechar", "Close")), None),
            Screen::Confirm { .. } => (
                Some(tr!("Desinstalar", "Uninstall")),
                Some(tr!("Cancelar", "Cancel")),
            ),
        };

        let glyph = match &self.screen {
            Screen::Done | Screen::Removed => Some((ICON_CHECK, t.accent)),
            Screen::Failed(_) => Some((ICON_WARNING, t.text)),
            _ => None,
        };
        let header = div()
            .flex()
            .items_center()
            .gap(px(space(4)))
            .child(match glyph {
                Some((g, color)) => div()
                    .size(px(64.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(self.icon_font)
                    .text_size(px(44.0))
                    .text_color(rgba(color))
                    .child(g)
                    .into_any_element(),
                None => img(self.logo.clone()).size(px(64.0)).into_any_element(),
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space(1)))
                    .child(
                        div()
                            .text_size(px(26.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .children(
                        subtitle.map(|line| div().text_color(rgba(t.text_secondary)).child(line)),
                    ),
            );

        let mut content = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(space(4)))
            .child(div().text_color(rgba(t.text_secondary)).child(body));

        if let Screen::Working { .. } = self.screen {
            content = content.child(
                div()
                    .mt(px(space(2)))
                    .h(px(4.0))
                    .w_full()
                    .rounded(px(2.0))
                    .bg(rgba(t.divider))
                    .child(
                        div()
                            .h_full()
                            .rounded(px(2.0))
                            .bg(rgba(t.accent))
                            .w(gpui::relative(self.shown.clamp(0.0, 1.0))),
                    ),
            );
        }
        if let Screen::Welcome { options } = self.screen {
            let folder = self.target.dir.display().to_string();
            let checkbox = if self.desktop_shortcut {
                ICON_CHECKBOX
            } else {
                ICON_CHECKBOX_EMPTY
            };
            content = content.child(
                div()
                    .id("options-toggle")
                    .cursor_pointer()
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.accent))
                    .child(if options {
                        tr!("Ocultar opções", "Hide options")
                    } else {
                        tr!("Opções", "Options")
                    })
                    .on_click(cx.listener(|v, _, _, cx| {
                        if let Screen::Welcome { options } = &mut v.screen {
                            *options = !*options;
                        }
                        cx.notify();
                    })),
            );
            if options {
                content = content.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(space(2)))
                        .p(px(space(3)))
                        .rounded(px(RADIUS * 2.0))
                        .border_1()
                        .border_color(rgba(t.divider))
                        .bg(rgba(t.layer))
                        .child(
                            div()
                                .id("desktop-shortcut")
                                .flex()
                                .items_center()
                                .gap(px(space(2)))
                                .cursor_pointer()
                                .child(
                                    div()
                                        .font_family(self.icon_font)
                                        .text_color(rgba(if self.desktop_shortcut {
                                            t.accent
                                        } else {
                                            t.text_secondary
                                        }))
                                        .child(checkbox),
                                )
                                .child(tr!(
                                    "Criar atalho na área de trabalho",
                                    "Create a desktop shortcut"
                                ))
                                .on_click(cx.listener(|v, _, _, cx| {
                                    v.desktop_shortcut = !v.desktop_shortcut;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(t.text_secondary))
                                .truncate()
                                .child(trf!("Pasta: {folder}", "Folder: {folder}")),
                        ),
                );
            }
        }
        if let Screen::Confirm { remove_data } = self.screen {
            let checkbox = if remove_data {
                ICON_CHECKBOX
            } else {
                ICON_CHECKBOX_EMPTY
            };
            content = content.child(
                div()
                    .id("remove-data")
                    .flex()
                    .items_center()
                    .gap(px(space(2)))
                    .cursor_pointer()
                    .child(
                        div()
                            .font_family(self.icon_font)
                            .text_color(rgba(if remove_data {
                                t.accent
                            } else {
                                t.text_secondary
                            }))
                            .child(checkbox),
                    )
                    .child(tr!(
                        "Apagar também configurações, abas salvas e histórico",
                        "Also delete settings, saved tabs and history"
                    ))
                    .on_click(cx.listener(|v, _, _, cx| {
                        if let Screen::Confirm { remove_data } = &mut v.screen {
                            *remove_data = !*remove_data;
                        }
                        cx.notify();
                    })),
            );
            if self.service {
                content = content.child(
                    div().text_size(px(CAPTION_SIZE)).text_color(rgba(t.text_secondary)).child(tr!(
                        "O serviço de busca no disco inteiro continua instalado. Para removê-lo, use \"Desativar a busca no disco inteiro\" na paleta do Pathdash antes.",
                        "The whole-disk search service stays installed. To remove it, use \"Turn off whole-disk search\" in Pathdash's palette first."
                    )),
                );
            }
        }

        let button = |id: &'static str, label: &str, strong: bool| {
            div()
                .id(SharedString::from(id))
                .h(px(32.0))
                .px(px(space(5)))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(RADIUS))
                .cursor_pointer()
                .when(strong, |d| {
                    d.bg(rgba(t.accent))
                        .text_color(rgba(if t.mode == Mode::Dark {
                            0x000000FF
                        } else {
                            0xFFFFFFFF
                        }))
                })
                .when(!strong, |d| {
                    d.border_1()
                        .border_color(rgba(t.divider))
                        .bg(rgba(t.layer))
                        .hover(|s| s.bg(rgba(t.hover)))
                })
                .child(label.to_owned())
        };
        let footer = div()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(space(2)))
            .child(
                div()
                    .flex_1()
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .child(trf!("Versão {new}", "Version {new}")),
            )
            .children(secondary.map(|label| {
                button("secondary", label, false)
                    .on_click(cx.listener(|v, _, _, cx| v.secondary(cx)))
            }))
            .children(primary.map(|label| {
                button("primary", label, true).on_click(cx.listener(|v, _, _, cx| v.primary(cx)))
            }));

        div()
            .id("setup")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|v, _, window, _| v.focus.focus(window)),
            )
            .size_full()
            .when(!self.mica, |d| d.bg(rgba(t.base)))
            .font_family(self.font)
            .text_size(px(BODY_SIZE))
            .text_color(rgba(t.text))
            .p(px(space(8)))
            .flex()
            .flex_col()
            .gap(px(space(6)))
            .child(header)
            .child(content)
            .child(footer)
    }
}

#[cfg(test)]
mod progress_tests {
    use super::{Step, goal_for};

    #[test]
    fn the_bar_follows_the_work_done() {
        // 4 files + shortcuts + registry = 6 units.
        assert_eq!(goal_for(Step::Copying(0, 4), 4, false), 0.0);
        assert_eq!(goal_for(Step::Copying(2, 4), 4, false), 2.0 / 6.0);
        assert_eq!(goal_for(Step::Shortcuts, 4, false), 4.0 / 6.0);
        assert_eq!(goal_for(Step::Registering, 4, false), 5.0 / 6.0);
        assert_eq!(goal_for(Step::Done, 4, false), 1.0);
        // Uninstall: 4 files + the entry = 5 units.
        assert_eq!(goal_for(Step::Copying(4, 4), 4, true), 4.0 / 5.0);
        assert_eq!(goal_for(Step::Registering, 4, true), 4.0 / 5.0);
        // Never past the end, never a division by zero.
        assert_eq!(goal_for(Step::Copying(0, 0), 0, true), 0.0);
        assert!(goal_for(Step::Copying(9, 4), 4, false) <= 1.0);
    }
}

//! Settings (the 2026 pattern of Linear, Notion, Slack, VS Code, Zed: a gear
//! at the bottom of the sidebar and Ctrl+, open a dialog with categories on
//! the left; "Sobre" is the last category). Every control shows the real
//! state and acts on it; nothing here is a copy of a setting.

use std::sync::{Arc, OnceLock};

use fx_core::{tr, trf};
use gpui::{
    Context, Image, ImageFormat, MouseButton, MouseDownEvent, SharedString, deferred, div, img,
    prelude::*, px,
};

use super::TabsView;
use crate::theme::{CAPTION_SIZE, Density, RADIUS, rgba, space};

const LOGO: &[u8] = include_bytes!("../../resources/logo-128.png");

/// The logo, made once (not copied and decoded again on every frame).
fn logo() -> Arc<Image> {
    static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
    IMAGE
        .get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO.to_vec())))
        .clone()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Appearance,
    Updates,
    Privacy,
    System,
    About,
}

impl Section {
    pub(super) const ALL: [Section; 6] = [
        Section::General,
        Section::Appearance,
        Section::Updates,
        Section::Privacy,
        Section::System,
        Section::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::General => tr!("Geral", "General"),
            Section::Appearance => tr!("Aparência", "Appearance"),
            Section::Updates => tr!("Atualizações", "Updates"),
            Section::Privacy => tr!("Privacidade", "Privacy"),
            Section::System => tr!("Sistema", "System"),
            Section::About => tr!("Sobre o Pathdash", "About Pathdash"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::General => "\u{E713}",
            Section::Appearance => "\u{E790}",
            Section::Updates => "\u{E895}",
            Section::Privacy => "\u{E72E}",
            Section::System => "\u{E7F8}",
            Section::About => "\u{E946}",
        }
    }
}

/// What a control does when used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Act {
    /// `true`: English. Choosing the language already in use does nothing.
    Language(bool),
    ConfirmRecycle,
    RestoreSession,
    Density(Density),
    Theme(crate::prefs::ThemePick),
    Hidden,
    Sidebar,
    AutoUpdate,
    CheckNow,
    /// Restart into the version ready.
    RestartToUpdate,
    CrashReports,
    DefaultManager(bool),
    IndexService(bool),
    HowTo,
    Link(&'static str),
}

/// The parts of System and Updates whose state lives in Windows (registry,
/// service manager, files): read on a worker thread each time Settings
/// opens and after every action. `None` while reading: nothing is assumed.
#[derive(Default)]
pub(super) struct Live {
    owner: Option<fx_shell::default_manager::Owner>,
    index: Option<fx_index::service::ServiceState>,
    last_check: Option<std::time::SystemTime>,
    /// The version downloaded, checked and waiting for Pathdash to close.
    ready: Option<String>,
    /// What runs now (its row shows it and offers no buttons meanwhile).
    busy: Option<Topic>,
    /// The outcome of the last check of this session.
    checked: Option<Result<(), String>>,
    /// The last action that went wrong, by row.
    error: Option<(Topic, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Topic {
    Manager,
    Index,
    Update,
}

/// The state line under a row's text.
pub(super) struct Status {
    pub text: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tone {
    /// On, done, ready.
    On,
    /// Off, or neutral information.
    Off,
    /// Working on it.
    Busy,
    Error,
}

fn status(tone: Tone, text: impl Into<String>) -> Option<Status> {
    Some(Status {
        text: text.into(),
        tone,
    })
}

/// A failed system action in words: a refused UAC prompt reads as such.
fn failure(e: &std::io::Error) -> String {
    // ERROR_CANCELLED: the user said no to the administrator prompt.
    if e.raw_os_error() == Some(1223) {
        tr!(
            "Cancelado: o Windows não recebeu a permissão de administrador.",
            "Cancelled: Windows did not get administrator permission."
        )
        .to_owned()
    } else {
        trf!("Não deu certo: {e}", "That did not work: {e}")
    }
}

/// "hoje às 18:20", "23/11/2026 17:46".
fn when(t: std::time::SystemTime) -> String {
    let offset = fx_shell::utc_offset_at(t);
    let now = std::time::SystemTime::now();
    let day = |t: std::time::SystemTime, o: i64| {
        t.duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| (d.as_secs() as i64 + o).div_euclid(86_400))
    };
    let full = fx_core::format::date_time(t, offset);
    if day(t, offset) == day(now, fx_shell::utc_offset_at(now)) {
        let hour = full.rsplit(' ').next().unwrap_or(&full).to_owned();
        trf!("hoje às {hour}", "today at {hour}")
    } else {
        full
    }
}

impl TabsView {
    /// Reads the state of System and Updates again (worker thread).
    pub(super) fn refresh_live(&mut self, cx: &mut Context<Self>) {
        let work = cx.background_executor().spawn(async move {
            use fx_shell::default_manager::{BACKUP, CLASSES, owner};
            let exe = std::env::current_exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            (
                owner(CLASSES, BACKUP, &exe),
                fx_index::service::service_state(),
                crate::update::last_check(),
                crate::update::ready().map(|(v, _, _)| v.to_string()),
            )
        });
        cx.spawn(async move |this, cx| {
            let (owner, index, last_check, ready) = work.await;
            let _ = this.update(cx, |view, cx| {
                let live = &mut view.live;
                live.owner = Some(owner);
                live.index = Some(index);
                live.last_check = last_check;
                live.ready = ready;
                cx.notify();
            });
        })
        .detach();
    }

    /// Ends `topic`'s work: its error (if any) stays on its row, and the
    /// real state is read again.
    fn finish(&mut self, topic: Topic, error: Option<String>, cx: &mut Context<Self>) {
        self.live.busy = None;
        self.live.error = error.map(|e| (topic, e));
        self.refresh_live(cx);
        cx.notify();
    }

    fn start(&mut self, topic: Topic) -> bool {
        if self.live.busy.is_some() {
            return false;
        }
        self.live.busy = Some(topic);
        if self.live.error.as_ref().is_some_and(|(t, _)| *t == topic) {
            self.live.error = None;
        }
        true
    }

    fn set_default_manager(&mut self, enable: bool, cx: &mut Context<Self>) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        if !self.start(Topic::Manager) {
            return;
        }
        let rx = fx_shell::launch::spawn_launch(move || {
            use fx_shell::default_manager::{BACKUP, CLASSES, enable as on, revert};
            if enable {
                // Another copy may own it: back to Explorer first, so the
                // backup keeps what Windows had, not that copy.
                if !fx_shell::default_manager::is_default(CLASSES, &exe.to_string_lossy()) {
                    revert(CLASSES, BACKUP)?;
                }
                on(CLASSES, BACKUP, &exe.to_string_lossy())
            } else {
                revert(CLASSES, BACKUP)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = crate::chan::next(rx).await.unwrap_or(Ok(()));
            let _ = this.update(cx, |view, cx| {
                view.finish(Topic::Manager, result.err().map(|e| failure(&e)), cx);
            });
        })
        .detach();
    }

    fn set_index_service(&mut self, install: bool, cx: &mut Context<Self>) {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("fx-indexd.exe")))
            .filter(|p| p.exists());
        let Some(exe) = exe else {
            self.live.error = Some((
                Topic::Index,
                tr!(
                    "O fx-indexd.exe não está na pasta do Pathdash. Reinstalar o Pathdash resolve.",
                    "fx-indexd.exe is not in the Pathdash folder. Reinstalling Pathdash fixes it."
                )
                .to_owned(),
            ));
            cx.notify();
            return;
        };
        if !self.start(Topic::Index) {
            return;
        }
        let args = if install { "install" } else { "uninstall" }.to_owned();
        let rx = fx_shell::launch(fx_shell::launch::Launch::Elevated { exe, args });
        cx.spawn(async move |this, cx| {
            let result = crate::chan::next(rx).await.unwrap_or(Ok(()));
            let _ = this.update(cx, |view, cx| {
                view.finish(Topic::Index, result.err().map(|e| failure(&e)), cx);
            });
        })
        .detach();
    }

    /// "Procurar agora": asks GitHub and, when there is a newer version,
    /// downloads and checks it (a thread of its own: network, D-066).
    fn check_now(&mut self, cx: &mut Context<Self>) {
        if !self.start(Topic::Update) {
            return;
        }
        let (tx, rx) = futures::channel::oneshot::channel();
        let _ = std::thread::Builder::new()
            .name("fx-update".into())
            .spawn(move || {
                let result = crate::update::check().and_then(|found| match found {
                    Some(u) => crate::update::prepare(&u),
                    None => Ok(()),
                });
                let _ = tx.send(result);
            });
        cx.spawn(async move |this, cx| {
            let result = rx.await.unwrap_or_else(|_| {
                Err(std::io::Error::other(tr!(
                    "não foi possível começar",
                    "could not start"
                )))
            });
            let _ = this.update(cx, |view, cx| {
                view.live.checked = Some(result.map_err(|e| e.to_string()));
                view.finish(Topic::Update, None, cx);
            });
        })
        .detach();
    }

    pub(super) fn open_settings(&mut self, section: Section, cx: &mut Context<Self>) {
        // A new section starts with no ring: the next Tab shows it on the
        // first control (it used to land on the second one).
        if self.settings != Some(section) {
            self.settings_focus = 0;
            self.settings_keyboard = false;
        }
        if self.settings.is_none() {
            self.refresh_live(cx);
        }
        self.settings = Some(section);
        cx.notify();
    }

    /// Tab / Shift+Tab: the next or previous control of the section.
    pub(super) fn settings_move_focus(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(section) = self.settings else {
            return;
        };
        let n = controls(&self.setting_rows(section)).len();
        if n > 0 && !self.settings_keyboard {
            // The first Tab shows where the focus is.
            self.settings_keyboard = true;
            cx.notify();
        } else if n > 0 {
            self.settings_focus = if forward {
                (self.settings_focus + 1) % n
            } else {
                (self.settings_focus + n - 1) % n
            };
            cx.notify();
        }
    }

    /// Space / Enter: uses the focused control (only once it is visible).
    pub(super) fn settings_activate(&mut self, cx: &mut Context<Self>) {
        let Some(section) = self.settings.filter(|_| self.settings_keyboard) else {
            return;
        };
        let act = controls(&self.setting_rows(section))
            .get(self.settings_focus)
            .map(|c| c.act);
        if let Some(act) = act {
            self.act(act, cx);
        }
    }

    pub(super) fn act(&mut self, act: Act, cx: &mut Context<Self>) {
        let tab = self.current();
        match act {
            Act::Language(english) => {
                if fx_core::locale::english() != english {
                    crate::i18n::switch();
                    cx.refresh_windows();
                }
            }
            Act::ConfirmRecycle => {
                crate::prefs::CONFIRM_RECYCLE.switch();
            }
            Act::RestoreSession => {
                crate::prefs::RESTORE_SESSION.switch();
            }
            Act::Density(d) => self.set_density(d, cx),
            Act::Theme(pick) => {
                crate::prefs::choose_theme(pick);
                cx.refresh_windows();
            }
            Act::Hidden => self.toggle_hidden(cx),
            Act::Sidebar => self.sidebar_open = !self.sidebar_open,
            Act::AutoUpdate => {
                crate::prefs::UPDATES.switch();
            }
            Act::CheckNow => self.check_now(cx),
            Act::RestartToUpdate => tab.update(cx, |view, cx| view.update_command(cx)),
            Act::CrashReports => {
                crate::prefs::CRASH_REPORTS.switch();
            }
            Act::DefaultManager(on) => self.set_default_manager(on, cx),
            Act::IndexService(on) => self.set_index_service(on, cx),
            Act::HowTo => {
                self.settings = None;
                self.open_guide(cx);
            }
            Act::Link(url) => tab.update(cx, |view, cx| view.open_url(url, cx)),
        }
        cx.notify();
    }
}

/// The control of a settings row.
pub(super) enum Control {
    None,
    Switch(bool, Act),
    /// A choice among options: label, selected, action.
    Segments(Vec<(&'static str, bool, Act)>),
    /// A value, such as the version (read-only).
    Value(String),
    Buttons(Vec<(&'static str, Act)>),
}

/// One settings row: what it is, what it does, how to change it. The same
/// data draws the screen and the screen reader's tree (review, D-066).
pub(super) struct Row {
    pub title: &'static str,
    pub detail: &'static str,
    /// The live state, under the text.
    pub status: Option<Status>,
    pub control: Control,
}

/// A control that takes the keyboard (each option of a choice is one).
pub(super) struct Focusable {
    pub label: String,
    pub kind: FocusKind,
    pub act: Act,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FocusKind {
    Switch(bool),
    Option(bool),
    Button,
}

/// The controls of `rows` in Tab order.
pub(super) fn controls(rows: &[Row]) -> Vec<Focusable> {
    let mut out = Vec::new();
    for row in rows {
        match &row.control {
            Control::None | Control::Value(_) => {}
            Control::Switch(on, act) => out.push(Focusable {
                label: row.title.to_owned(),
                kind: FocusKind::Switch(*on),
                act: *act,
            }),
            Control::Segments(options) => {
                for (label, selected, act) in options {
                    out.push(Focusable {
                        label: format!("{}: {label}", row.title),
                        kind: FocusKind::Option(*selected),
                        act: *act,
                    });
                }
            }
            Control::Buttons(buttons) => {
                for (label, act) in buttons {
                    out.push(Focusable {
                        label: (*label).to_owned(),
                        kind: FocusKind::Button,
                        act: *act,
                    });
                }
            }
        }
    }
    out
}

impl TabsView {
    /// The rows of `section`, with the real state of each setting.
    pub(super) fn setting_rows(&self, section: Section) -> Vec<Row> {
        let english = fx_core::locale::english();
        let density = self.look.density;
        match section {
            Section::General => vec![
                Row {
                    title: tr!("Idioma", "Language"),
                    detail: tr!(
                        "O padrão segue o idioma do Windows.",
                        "By default it follows the Windows language."
                    ),
                    status: None,
                    control: Control::Segments(vec![
                        ("Português", !english, Act::Language(false)),
                        ("English", english, Act::Language(true)),
                    ]),
                },
                Row {
                    title: tr!(
                        "Confirmar antes de enviar para a Lixeira",
                        "Confirm before sending to the Recycle Bin"
                    ),
                    detail: tr!(
                        "Excluir de vez (Shift+Delete) sempre pergunta.",
                        "Deleting for good (Shift+Delete) always asks."
                    ),
                    status: None,
                    control: Control::Switch(
                        crate::prefs::CONFIRM_RECYCLE.get(),
                        Act::ConfirmRecycle,
                    ),
                },
                Row {
                    title: tr!(
                        "Reabrir abas e janelas ao iniciar",
                        "Reopen tabs and windows at start"
                    ),
                    detail: tr!(
                        "Vale na próxima abertura do Pathdash.",
                        "Applies the next time Pathdash opens."
                    ),
                    status: None,
                    control: Control::Switch(
                        crate::prefs::RESTORE_SESSION.get(),
                        Act::RestoreSession,
                    ),
                },
            ],
            Section::Appearance => vec![
                Row {
                    title: tr!("Densidade das linhas", "Row density"),
                    detail: tr!(
                        "Ctrl+1, Ctrl+2 e Ctrl+3 também trocam.",
                        "Ctrl+1, Ctrl+2 and Ctrl+3 switch it too."
                    ),
                    status: None,
                    control: Control::Segments(vec![
                        (
                            tr!("Compacta", "Compact"),
                            density == Density::Compact,
                            Act::Density(Density::Compact),
                        ),
                        (
                            tr!("Padrão", "Standard"),
                            density == Density::Standard,
                            Act::Density(Density::Standard),
                        ),
                        (
                            tr!("Confortável", "Comfortable"),
                            density == Density::Comfortable,
                            Act::Density(Density::Comfortable),
                        ),
                    ]),
                },
                Row {
                    title: tr!("Mostrar itens ocultos", "Show hidden items"),
                    detail: tr!(
                        "Arquivos ocultos e de sistema (Ctrl+H).",
                        "Hidden and system files (Ctrl+H)."
                    ),
                    status: None,
                    control: Control::Switch(self.look.show_hidden, Act::Hidden),
                },
                Row {
                    title: tr!("Barra lateral", "Sidebar"),
                    detail: tr!(
                        "Acesso rápido e unidades (Ctrl+B).",
                        "Quick access and drives (Ctrl+B)."
                    ),
                    status: None,
                    control: Control::Switch(self.sidebar_open, Act::Sidebar),
                },
                Row {
                    title: tr!("Tema", "Theme"),
                    detail: tr!(
                        "O alto contraste do Windows sempre tem prioridade.",
                        "Windows high contrast always comes first."
                    ),
                    status: None,
                    control: {
                        use crate::prefs::ThemePick;
                        let pick = crate::prefs::theme();
                        Control::Segments(vec![
                            (
                                tr!("Escuro", "Dark"),
                                pick == ThemePick::Dark,
                                Act::Theme(ThemePick::Dark),
                            ),
                            (
                                tr!("Claro", "Light"),
                                pick == ThemePick::Light,
                                Act::Theme(ThemePick::Light),
                            ),
                            (
                                tr!("Seguir o Windows", "Follow Windows"),
                                pick == ThemePick::System,
                                Act::Theme(ThemePick::System),
                            ),
                        ])
                    },
                },
            ],
            Section::Updates => self.update_rows(),
            Section::Privacy => vec![
                Row {
                    title: tr!("Relatórios de falha", "Crash reports"),
                    detail: tr!(
                        "Se o Pathdash fechar com uma falha, guarda um relatório sem caminhos nem nomes neste computador. Nada é enviado sem você.",
                        "If Pathdash closes with a crash, it keeps a report with no paths or names on this computer. Nothing is sent without you."
                    ),
                    status: None,
                    control: Control::Switch(crate::prefs::CRASH_REPORTS.get(), Act::CrashReports),
                },
                Row {
                    title: tr!("Dados que o Pathdash envia", "Data Pathdash sends"),
                    detail: tr!(
                        "Nenhum. A verificação de atualizações só consulta a lista de versões no GitHub.",
                        "None. The update check only reads the list of versions on GitHub."
                    ),
                    status: None,
                    control: Control::None,
                },
            ],
            Section::System => self.system_rows(),
            Section::About => vec![
                Row {
                    title: tr!("Licença", "License"),
                    detail: tr!(
                        "Software livre, sob as licenças MIT ou Apache 2.0. Feito em Rust, nativo do Windows.",
                        "Free software, under the MIT or Apache 2.0 licenses. Made in Rust, native to Windows."
                    ),
                    status: None,
                    control: Control::None,
                },
                Row {
                    title: tr!("Ajuda e projeto", "Help and project"),
                    detail: tr!(
                        "O guia de atalhos, o que mudou em cada versão e o código no GitHub.",
                        "The shortcut guide, what changed in each version and the code on GitHub."
                    ),
                    status: None,
                    control: Control::Buttons(vec![
                        (tr!("Como usar (F1)", "How to use (F1)"), Act::HowTo),
                        (
                            tr!("Novidades", "What's new"),
                            Act::Link("https://github.com/heapwraith/fx/blob/main/CHANGELOG.md"),
                        ),
                        (
                            tr!("Código e sugestões", "Code and feedback"),
                            Act::Link("https://github.com/heapwraith/fx"),
                        ),
                        (
                            tr!("Relatar um problema", "Report a problem"),
                            Act::Link("https://github.com/heapwraith/fx/issues/new/choose"),
                        ),
                    ]),
                },
            ],
        }
    }

    /// The error of `topic`'s last action, as its status.
    fn error_of(&self, topic: Topic) -> Option<Status> {
        match &self.live.error {
            Some((t, e)) if *t == topic => status(Tone::Error, e.clone()),
            _ => None,
        }
    }

    fn update_rows(&self) -> Vec<Row> {
        let version = crate::VERSION.split('+').next().unwrap_or(crate::VERSION);
        let live = &self.live;
        let busy = live.busy == Some(Topic::Update);
        let last = live.last_check.map(when);
        let (state, buttons) = if busy {
            (
                status(
                    Tone::Busy,
                    tr!(
                        "Procurando no GitHub; uma versão nova é baixada e conferida…",
                        "Checking GitHub; a new version is downloaded and verified…"
                    ),
                ),
                vec![],
            )
        } else if let Some(v) = &live.ready {
            (
                status(
                    Tone::On,
                    trf!(
                        "Versão {v} baixada e conferida. Entra quando você fechar o Pathdash.",
                        "Version {v} downloaded and verified. It applies when you close Pathdash."
                    ),
                ),
                vec![(tr!("Reiniciar agora", "Restart now"), Act::RestartToUpdate)],
            )
        } else {
            let check = vec![(tr!("Procurar agora", "Check now"), Act::CheckNow)];
            let text = match (&live.checked, &last) {
                (Some(Err(e)), _) => {
                    return self.update_rows_with(
                        version,
                        status(
                            Tone::Error,
                            trf!("Não foi possível verificar: {e}", "Could not check: {e}"),
                        ),
                        check,
                    );
                }
                (Some(Ok(())), Some(at)) | (None, Some(at)) => trf!(
                    "Você está na versão mais recente. Última verificação: {at}.",
                    "You are on the latest version. Last checked {at}."
                ),
                (Some(Ok(())), None) => tr!(
                    "Você está na versão mais recente.",
                    "You are on the latest version."
                )
                .to_owned(),
                (None, None) => tr!(
                    "Ainda não verificado neste computador.",
                    "Not checked on this computer yet."
                )
                .to_owned(),
            };
            (status(Tone::Off, text), check)
        };
        self.update_rows_with(version, state, buttons)
    }

    fn update_rows_with(
        &self,
        version: &str,
        state: Option<Status>,
        buttons: Vec<(&'static str, Act)>,
    ) -> Vec<Row> {
        vec![
            Row {
                title: tr!("Versão instalada", "Installed version"),
                detail: "",
                status: None,
                control: Control::Value(version.to_owned()),
            },
            Row {
                title: tr!("Procurar atualizações", "Check for updates"),
                detail: tr!(
                    "Consulta as versões publicadas no GitHub.",
                    "Looks at the versions published on GitHub."
                ),
                status: state,
                control: Control::Buttons(buttons),
            },
            Row {
                title: tr!("Atualizações automáticas", "Automatic updates"),
                detail: tr!(
                    "Uma vez por dia, baixa a versão nova em segundo plano, confere e instala quando você fecha o Pathdash.",
                    "Once a day, downloads the new version in the background, verifies it and installs it when you close Pathdash."
                ),
                status: None,
                control: Control::Switch(crate::prefs::UPDATES.get(), Act::AutoUpdate),
            },
        ]
    }

    fn system_rows(&self) -> Vec<Row> {
        use fx_index::service::ServiceState;
        use fx_shell::default_manager::Owner;
        let live = &self.live;
        let checking = || (status(Tone::Busy, tr!("Verificando…", "Checking…")), vec![]);
        let (manager_state, manager_buttons) = if live.busy == Some(Topic::Manager) {
            (status(Tone::Busy, tr!("Aplicando…", "Applying…")), vec![])
        } else {
            match &live.owner {
                None => checking(),
                Some(Owner::This) => (
                    status(
                        Tone::On,
                        tr!(
                            "Ativo: pastas, unidades e Win+E abrem no Pathdash.",
                            "On: folders, drives and Win+E open in Pathdash."
                        ),
                    ),
                    vec![(
                        tr!("Voltar ao Explorador", "Back to Explorer"),
                        Act::DefaultManager(false),
                    )],
                ),
                Some(Owner::Explorer) => (
                    status(
                        Tone::Off,
                        tr!(
                            "Desativado: o Explorador abre as pastas.",
                            "Off: Explorer opens folders."
                        ),
                    ),
                    vec![(
                        tr!("Usar o Pathdash", "Use Pathdash"),
                        Act::DefaultManager(true),
                    )],
                ),
                Some(Owner::Other(command)) => (
                    status(
                        Tone::Off,
                        trf!(
                            "Outra cópia do Pathdash está como padrão: {command}",
                            "Another copy of Pathdash is the default: {command}"
                        ),
                    ),
                    vec![
                        (
                            tr!("Usar esta cópia", "Use this copy"),
                            Act::DefaultManager(true),
                        ),
                        (
                            tr!("Voltar ao Explorador", "Back to Explorer"),
                            Act::DefaultManager(false),
                        ),
                    ],
                ),
            }
        };
        let (index_state, index_buttons) = if live.busy == Some(Topic::Index) {
            (
                status(
                    Tone::Busy,
                    tr!(
                        "Aguardando a permissão de administrador do Windows…",
                        "Waiting for Windows administrator permission…"
                    ),
                ),
                vec![],
            )
        } else {
            match live.index {
                None => checking(),
                Some(ServiceState::Running) => (
                    status(
                        Tone::On,
                        tr!(
                            "Ativa: o serviço do índice está rodando e a busca cobre os discos NTFS.",
                            "On: the index service is running and search covers the NTFS disks."
                        ),
                    ),
                    vec![(tr!("Desativar", "Turn off"), Act::IndexService(false))],
                ),
                Some(ServiceState::NotInstalled) => (
                    status(
                        Tone::Off,
                        tr!(
                            "Desativada: a busca procura dentro da pasta aberta.",
                            "Off: search looks inside the open folder."
                        ),
                    ),
                    vec![(tr!("Ativar", "Turn on"), Act::IndexService(true))],
                ),
                Some(ServiceState::Stopped | ServiceState::Other) => (
                    status(
                        Tone::Error,
                        tr!(
                            "O serviço está instalado, mas parado: a busca não usa o índice.",
                            "The service is installed but stopped: search does not use the index."
                        ),
                    ),
                    vec![
                        (
                            tr!("Ativar de novo", "Turn on again"),
                            Act::IndexService(true),
                        ),
                        (tr!("Desativar", "Turn off"), Act::IndexService(false)),
                    ],
                ),
            }
        };
        vec![
            Row {
                title: tr!("Gerenciador de arquivos padrão", "Default file manager"),
                detail: tr!(
                    "Pastas, unidades e Win+E abrem no Pathdash. Voltar ao Explorador restaura o que havia.",
                    "Folders, drives and Win+E open in Pathdash. Going back to Explorer restores what was there."
                ),
                status: self.error_of(Topic::Manager).or(manager_state),
                control: Control::Buttons(manager_buttons),
            },
            Row {
                title: tr!("Busca no disco inteiro", "Whole-disk search"),
                detail: tr!(
                    "Um serviço do Windows mantém o índice dos discos NTFS. Ativar e desativar pedem permissão de administrador.",
                    "A Windows service keeps the index of the NTFS disks. Turning it on or off asks for administrator permission."
                ),
                status: self.error_of(Topic::Index).or(index_state),
                control: Control::Buttons(index_buttons),
            },
        ]
    }

    /// Settings as a screen reader reads them: the sections, then each row
    /// and its controls; the focus on the focused control. `acts[k]` is what
    /// a click on item `k` does.
    pub(super) fn settings_overlay(
        &self,
        section: Section,
    ) -> (crate::a11y::Overlay, Vec<Option<super::WindowAct>>) {
        use crate::a11y::{Item, Overlay};
        let rows = self.setting_rows(section);
        let mut items = Vec::new();
        let mut acts = Vec::new();
        for s in Section::ALL {
            items.push(Item::Choice {
                label: s.label().to_owned(),
                description: None,
                selected: s == section,
            });
            acts.push(Some(super::WindowAct::Section(s)));
        }
        if section == Section::About {
            items.push(Item::Text(about_line()));
            acts.push(None);
        }
        let mut focus = None;
        let mut control = 0usize;
        for row in &rows {
            let text = match (row.title.is_empty(), row.detail.is_empty()) {
                (true, true) => None,
                (_, true) => Some(row.title.to_owned()),
                (true, _) => Some(row.detail.to_owned()),
                _ => Some(format!("{}. {}", row.title, row.detail)),
            };
            if let Some(text) = text {
                items.push(Item::Text(text));
                acts.push(None);
            }
            if let Control::Value(value) = &row.control {
                items.push(Item::Text(value.clone()));
                acts.push(None);
            }
            if let Some(st) = &row.status {
                items.push(Item::Text(st.text.clone()));
                acts.push(None);
            }
            for c in controls(std::slice::from_ref(row)) {
                if control == self.settings_focus {
                    focus = Some(items.len());
                }
                items.push(match c.kind {
                    FocusKind::Switch(on) => Item::Switch { label: c.label, on },
                    FocusKind::Option(selected) => Item::Radio {
                        label: c.label,
                        checked: selected,
                    },
                    FocusKind::Button => Item::Button(c.label),
                });
                acts.push(Some(super::WindowAct::Settings(c.act)));
                control += 1;
            }
        }
        let overlay = Overlay {
            title: tr!("Configurações", "Settings").to_owned(),
            description: Some(trf!(
                "{}. Tab passa entre os controles, Espaço usa, ↑ e ↓ trocam de seção, Esc fecha.",
                "{}. Tab moves between controls, Space uses one, ↑ and ↓ change section, Esc closes.",
                section.label()
            )),
            items,
            focus,
            positions: None,
            modal: true,
        };
        (overlay, acts)
    }

    pub(super) fn render_settings(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let section = self.settings?;
        let t = self.look.theme;
        let icon_font = self.look.icon_font;

        let nav: Vec<_> = Section::ALL
            .iter()
            .map(|&s| {
                let active = s == section;
                div()
                    .id(SharedString::from(format!("settings-nav-{s:?}")))
                    .h(px(32.0))
                    .px(px(space(3)))
                    .flex()
                    .items_center()
                    .gap(px(space(3)))
                    .rounded(px(RADIUS))
                    .cursor_pointer()
                    .when(active, |d| d.bg(rgba(t.selected)))
                    .when(!active, |d| d.hover(|h| h.bg(rgba(t.hover))))
                    .child(
                        div()
                            .font_family(icon_font)
                            .text_size(px(14.0))
                            .child(s.icon()),
                    )
                    .child(s.label())
                    .on_click(cx.listener(move |view, _, _, cx| view.open_settings(s, cx)))
            })
            .collect();

        let rows = self.setting_rows(section);
        // No ring until the keyboard is used (focus-visible).
        let focus = if self.settings_keyboard {
            self.settings_focus
        } else {
            usize::MAX
        };
        // The keyboard focus: a 2 px accent ring; the others get the same
        // ring, transparent, so nothing moves when it changes.
        let ring = move |focused: bool, el: gpui::AnyElement| {
            div()
                .rounded(px(RADIUS + 2.0))
                .p(px(1.0))
                .border_2()
                .border_color(rgba(if focused { t.accent } else { 0x0000_0000 }))
                .child(el)
                .into_any_element()
        };
        // The whole row takes the click (a bigger target, WCAG 2.5.8).
        let switch = |id: SharedString, on: bool| {
            div()
                .id(id)
                .w(px(40.0))
                .h(px(20.0))
                .rounded_full()
                .cursor_pointer()
                .flex()
                .items_center()
                .px(px(3.0))
                .when(on, |d| d.bg(rgba(t.accent)).justify_end())
                .when(!on, |d| d.border_1().border_color(rgba(t.text_secondary)))
                .child(div().size(px(12.0)).rounded_full().bg(rgba(if on {
                    t.base
                } else {
                    t.text_secondary
                })))
                .into_any_element()
        };
        let button = |id: SharedString, label: &'static str, act: Act, cx: &mut Context<Self>| {
            div()
                .id(id)
                .h(px(30.0))
                .px(px(space(3)))
                .flex()
                .items_center()
                .rounded(px(RADIUS))
                .border_1()
                .border_color(rgba(t.divider))
                .bg(rgba(t.layer))
                .cursor_pointer()
                .hover(|h| h.bg(rgba(t.hover)))
                .child(label)
                .on_click(cx.listener(move |view, _, _, cx| view.act(act, cx)))
                .into_any_element()
        };
        let mut next = 0usize;
        let mut body = div().flex().flex_col();
        if section == Section::About {
            body = body.child(about_header(t));
        }
        for (r, row) in rows.into_iter().enumerate() {
            // Two or more buttons go under the text, across the row (they do
            // not fit beside it in a narrow window).
            let stacked = matches!(&row.control, Control::Buttons(b) if b.len() > 1);
            let row_act = match row.control {
                Control::Switch(_, act) => Some(act),
                _ => None,
            };
            let control: gpui::AnyElement = match row.control {
                Control::None => div().into_any_element(),
                Control::Value(v) => div()
                    .text_color(rgba(t.text_secondary))
                    .child(v)
                    .into_any_element(),
                Control::Switch(on, _) => {
                    let id = SharedString::from(format!("set-{r}-switch"));
                    let el = ring(next == focus, switch(id, on));
                    next += 1;
                    el
                }
                Control::Segments(options) => {
                    let mut seg = div()
                        .flex()
                        .rounded(px(RADIUS))
                        .border_1()
                        .border_color(rgba(t.divider))
                        .overflow_hidden();
                    for (k, (label, selected, act)) in options.into_iter().enumerate() {
                        let focused = next == focus;
                        next += 1;
                        seg = seg.child(
                            div()
                                .id(SharedString::from(format!("set-{r}-{k}")))
                                .h(px(28.0))
                                .px(px(space(3)))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .border_2()
                                .border_color(rgba(if focused { t.accent } else { 0x0000_0000 }))
                                .when(selected, |d| {
                                    d.bg(rgba(t.selected)).text_color(rgba(t.text))
                                })
                                .when(!selected, |d| {
                                    d.text_color(rgba(t.text_secondary))
                                        .hover(|h| h.bg(rgba(t.hover)))
                                })
                                .child(label)
                                .on_click(cx.listener(move |view, _, _, cx| view.act(act, cx))),
                        );
                    }
                    seg.into_any_element()
                }
                Control::Buttons(buttons) => {
                    let mut line = div().flex().flex_wrap().items_center().gap(px(space(2)));
                    for (k, (label, act)) in buttons.into_iter().enumerate() {
                        let id = SharedString::from(format!("set-{r}-b{k}"));
                        line = line.child(ring(next == focus, button(id, label, act, cx)));
                        next += 1;
                    }
                    line.into_any_element()
                }
            };
            let state = row.status.map(|st| {
                let dot = match st.tone {
                    Tone::On => t.accent,
                    Tone::Off | Tone::Busy => t.text_secondary,
                    Tone::Error => t.danger,
                };
                div()
                    .pt(px(space(1)))
                    .flex()
                    .items_start()
                    .gap(px(space(2)))
                    .text_size(px(CAPTION_SIZE))
                    .child(
                        // An outlined dot while working or off, filled when
                        // on or wrong: the state reads at a glance.
                        div()
                            .flex_none()
                            .mt(px(4.0))
                            .size(px(8.0))
                            .rounded_full()
                            .when(matches!(st.tone, Tone::On | Tone::Error), |d| {
                                d.bg(rgba(dot))
                            })
                            .when(matches!(st.tone, Tone::Off | Tone::Busy), |d| {
                                d.border_1().border_color(rgba(dot))
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(rgba(if st.tone == Tone::Busy {
                                t.text_secondary
                            } else {
                                t.text
                            }))
                            .child(st.text),
                    )
            });
            let text = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .when(!row.title.is_empty(), |d| d.child(row.title))
                .when(!row.detail.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .child(row.detail),
                    )
                })
                .children(state);
            let line = div()
                .id(SharedString::from(format!("set-row-{r}")))
                .py(px(space(3)))
                .flex()
                .border_b_1()
                .border_color(rgba(t.divider));
            let line = if stacked {
                line.flex_col().gap(px(space(2))).child(text).child(control)
            } else {
                line.items_center()
                    .gap(px(space(4)))
                    .child(text)
                    .child(div().flex_none().child(control))
            };
            let line = match row_act {
                Some(act) => line
                    .cursor_pointer()
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.settings_keyboard = false;
                        view.act(act, cx)
                    })),
                None => line,
            };
            body = body.child(line);
        }

        // Never wider or taller than the window.
        let card = div()
            .id("settings")
            .w(px(780.0))
            .max_w(gpui::relative(0.94))
            .h(px(520.0))
            .max_h(gpui::relative(0.9))
            .flex()
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(200.0))
                    .flex_none()
                    .p(px(space(2)))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .bg(rgba(t.layer))
                    .border_r_1()
                    .border_color(rgba(t.divider))
                    .child(
                        div()
                            .px(px(space(3)))
                            .py(px(space(3)))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(tr!("Configurações", "Settings")),
                    )
                    .children(nav)
                    .child(div().flex_1())
                    .child(
                        div()
                            .px(px(space(3)))
                            .py(px(space(2)))
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(tr!("↑ ↓ trocam de seção", "↑ ↓ change section"))
                            .child(tr!(
                                "Tab e Espaço usam os controles",
                                "Tab and Space use controls"
                            ))
                            .child(tr!("Esc fecha", "Esc closes")),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .child(
                        div()
                            .id("settings-body")
                            .size_full()
                            .p(px(space(6)))
                            .overflow_y_scroll()
                            .track_scroll(&self.settings_scroll.handle)
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .pb(px(space(2)))
                                    .child(section.label()),
                            )
                            .child(body),
                    )
                    .child(crate::scrollbar::vertical(&self.settings_scroll, &t)),
            );
        Some(
            deferred(crate::motion::enter(
                "settings-in",
                div()
                    .id("settings-backdrop")
                    // The closing click never reaches the list behind.
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(crate::theme::scrim(&t)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.settings = None;
                            cx.notify();
                        }),
                    )
                    .child(card),
            ))
            .with_priority(2),
        )
    }
}

/// "fx", the slogan and the version, at the top of "Sobre o fx".
fn about_header(t: crate::theme::Theme) -> gpui::AnyElement {
    div()
        .py(px(space(4)))
        .flex()
        .items_center()
        .gap(px(space(4)))
        // The logo keeps its size; the text shrinks and wraps (the long
        // slogan used to squeeze the logo to nothing and run off the card).
        .child(img(logo()).flex_none().size(px(64.0)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(space(1)))
                .child(
                    div()
                        .text_size(px(22.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Pathdash"),
                )
                .child(
                    div()
                        .text_color(rgba(t.text_secondary))
                        .child(super::guide::slogan()),
                )
                .child(
                    div()
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(version_text()),
                ),
        )
        .into_any_element()
}

/// "Versão 0.2.0 · compilação abc1234".
fn version_text() -> String {
    let version = crate::VERSION.split('+').next().unwrap_or(crate::VERSION);
    match crate::VERSION.split_once('+') {
        Some((_, c)) => trf!(
            "Versão {version} · compilação {c}",
            "Version {version} · build {c}"
        ),
        None => trf!("Versão {version}", "Version {version}"),
    }
}

/// The header of "Sobre o fx" as one line, for screen readers.
fn about_line() -> String {
    format!("Pathdash. {}. {}.", super::guide::slogan(), version_text())
}

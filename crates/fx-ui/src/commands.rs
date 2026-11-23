//! The command palette's commands (task T1.24). Most commands *are* their
//! shortcut: running one delivers the same keystroke to the window or the
//! active tab, so the palette and the keyboard can never disagree. A few have
//! no key and act directly.

use crate::folder_view::{ImageAction, MediaCol};
use fx_core::SortKey;
use fx_core::grouping::GroupBy;
use fx_core::tr;
use fx_shell::images::ImageKind;

/// Who handles a command's keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// `TabsView` (tabs, sidebar, window-wide view settings).
    Window,
    /// The active tab (`FolderView`).
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Key(Target, &'static str),
    ToggleGrid,
    Sort(SortKey),
    /// Shows the version (with the commit it was built from) in the status.
    About,
    /// Installs (`true`) or removes the index service, with the UAC prompt.
    IndexService(bool),
    /// Makes fx the default file manager (`true`) or goes back to Explorer.
    DefaultManager(bool),
    /// Switches the interface between Portuguese and English (T3.7).
    Language,
    /// Cloud files (T3.9): keep on this device (`true`) or free up space.
    CloudPin(bool),
    /// Opens a GitHub issue with the last crash report (T3.10).
    ReportCrash,
    /// Turns the local crash reports on or off.
    CrashReports,
    /// Installs the update found, or checks now (T4.1).
    Update,
    /// Turns the daily update check on or off.
    UpdateChecks,
    /// Asks (or not) before sending items to the Recycle Bin.
    ConfirmRecycle,
    /// T6.11: compares the two panes of the dual view.
    Compare,
    /// T6.16: copies of the selected images.
    Image(crate::folder_view::ImageAction),
    /// T6.10: "Agrupar por" and the media columns.
    Group(GroupBy),
    MediaColumn(MediaCol),
    /// T6.13: asks a name and saves this window's tabs as a workspace.
    SaveWorkspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// Portuguese and English.
    pub labels: (&'static str, &'static str),
    pub action: Action,
}

impl Command {
    /// The label in the interface language.
    pub fn label(&self) -> &'static str {
        if fx_core::locale::english() {
            self.labels.1
        } else {
            self.labels.0
        }
    }

    /// A glyph (Segoe Fluent Icons / MDL2 Assets, present in both) that says
    /// what the command does: a palette where every row has the same icon
    /// gives the eye nothing to find (review of the interface, D-067).
    pub fn icon(&self) -> &'static str {
        match self.action {
            Action::Key(_, keys) => match keys {
                "ctrl-f" | "ctrl-k" => "\u{E721}",            // Search
                "space" => "\u{E7B3}",                        // RedEye (preview)
                "ctrl-shift-n" => "\u{E8F4}",                 // NewFolder
                "f2" => "\u{E8AC}",                           // Rename
                "delete" => "\u{E74D}",                       // Delete (bin)
                "shift-delete" | "ctrl-w" => "\u{E711}",      // Cancel
                "ctrl-c" => "\u{E8C8}",                       // Copy
                "ctrl-x" => "\u{E8C6}",                       // Cut
                "ctrl-v" => "\u{E77F}",                       // Paste
                "ctrl-shift-c" => "\u{E71B}",                 // Link
                "ctrl-z" => "\u{E7A7}",                       // Undo
                "ctrl-y" => "\u{E7A6}",                       // Redo
                "ctrl-a" => "\u{E8B3}",                       // SelectAll
                "alt-enter" => "\u{E946}",                    // Info
                "shift-enter" => "\u{E7AC}",                  // OpenWith
                "shift-f10" => "\u{E700}",                    // Menu
                "ctrl-l" => "\u{E70F}",                       // Edit
                "alt-up" => "\u{E74A}",                       // Up
                "alt-left" => "\u{E72B}",                     // Back
                "alt-right" => "\u{E72A}",                    // Forward
                "ctrl-t" => "\u{E710}",                       // Add
                "ctrl-tab" | "ctrl-shift-tab" => "\u{E7C4}",  // TaskView
                "ctrl-1" | "ctrl-2" | "ctrl-3" => "\u{E8FD}", // BulletedList
                "ctrl-b" => "\u{E8A0}",                       // OpenPane
                "ctrl-h" => "\u{E890}",                       // View
                "ctrl-shift-h" => "\u{E81C}",
                "ctrl-shift-m" => "\u{E9D9}", // Diagnostic (chart)
                "ctrl-shift-l" => "\u{E823}", // Recent (clock)
                "ctrl-shift-d" => "\u{E8C8}", // Copy (two pages)
                "alt-shift-p" => "\u{E8A0}",  // OpenPane                 // History
                "f1" => "\u{E897}",           // Help
                "ctrl-j" => "\u{E756}",       // CommandPrompt
                _ => "\u{E756}",
            },
            Action::ToggleGrid => "\u{E80A}", // ViewAll
            Action::Sort(_) => "\u{E8CB}",    // Sort
            Action::About => "\u{E946}",      // Info
            Action::IndexService(_) => "\u{E721}",
            Action::DefaultManager(_) => "\u{E8B7}", // Folder
            Action::Language => "\u{E774}",          // Globe
            Action::CloudPin(_) => "\u{E753}",       // Cloud
            Action::ReportCrash | Action::CrashReports => "\u{E7BA}", // Warning
            Action::Update | Action::UpdateChecks => "\u{E895}", // Sync
            Action::ConfirmRecycle => "\u{E74D}",
            Action::SaveWorkspace => "\u{E74E}",  // Save
            Action::Compare => "\u{E8F9}",        // Switch
            Action::Image(_) => "\u{EB9F}",       // Photo
            Action::Group(_) => "\u{F168}",       // GroupList
            Action::MediaColumn(_) => "\u{E71D}", // AllApps (columns)
        }
    }

    /// The shortcut as shown next to the command ("Ctrl+Shift+N").
    pub fn shortcut(&self) -> Option<String> {
        match self.action {
            Action::Key(_, keys) => Some(shortcut_label(keys)),
            _ => None,
        }
    }
}

type Labels = (&'static str, &'static str);

const fn window(labels: Labels, keys: &'static str) -> Command {
    Command {
        labels,
        action: Action::Key(Target::Window, keys),
    }
}

const fn tab(labels: Labels, keys: &'static str) -> Command {
    Command {
        labels,
        action: Action::Key(Target::Tab, keys),
    }
}

const fn direct(labels: Labels, action: Action) -> Command {
    Command { labels, action }
}

pub const COMMANDS: &[Command] = &[
    tab(("Buscar no disco", "Search the disk"), "ctrl-f"),
    tab(
        ("Pré-visualizar (Quick Look)", "Preview (Quick Look)"),
        "space",
    ),
    tab(("Nova pasta", "New folder"), "ctrl-shift-n"),
    tab(("Renomear", "Rename"), "f2"),
    tab(("Enviar para a Lixeira", "Send to Recycle Bin"), "delete"),
    tab(
        ("Excluir permanentemente", "Delete permanently"),
        "shift-delete",
    ),
    tab(("Copiar", "Copy"), "ctrl-c"),
    tab(("Recortar", "Cut"), "ctrl-x"),
    tab(("Colar", "Paste"), "ctrl-v"),
    tab(("Copiar caminho", "Copy path"), "ctrl-shift-c"),
    tab(("Desfazer", "Undo"), "ctrl-z"),
    tab(("Refazer", "Redo"), "ctrl-y"),
    tab(("Selecionar tudo", "Select all"), "ctrl-a"),
    tab(
        (
            "Mapa do disco (o que ocupa espaço)",
            "Disk map (what takes space)",
        ),
        "ctrl-shift-m",
    ),
    tab(
        (
            "Linha do tempo (o que mudou nos discos)",
            "Timeline (what changed on the disks)",
        ),
        "ctrl-shift-l",
    ),
    tab(
        (
            "Arquivos repetidos (nesta pasta e subpastas)",
            "Repeated files (here and below)",
        ),
        "ctrl-shift-d",
    ),
    tab(("Painel de detalhes", "Details panel"), "alt-shift-p"),
    direct(
        ("Imagens: copiar como JPG", "Images: copy as JPG"),
        Action::Image(ImageAction::To(ImageKind::Jpeg)),
    ),
    direct(
        ("Imagens: copiar como PNG", "Images: copy as PNG"),
        Action::Image(ImageAction::To(ImageKind::Png)),
    ),
    direct(
        (
            "Imagens: cópia com metade do tamanho",
            "Images: copy at half size",
        ),
        Action::Image(ImageAction::Half),
    ),
    direct(
        (
            "Imagens: renomear pela data da foto",
            "Images: rename by date taken",
        ),
        Action::Image(ImageAction::RenameByDate),
    ),
    direct(
        ("Agrupar por tipo", "Group by type"),
        Action::Group(GroupBy::Type),
    ),
    direct(
        ("Agrupar por data de modificação", "Group by date modified"),
        Action::Group(GroupBy::Date),
    ),
    direct(
        ("Agrupar por tamanho", "Group by size"),
        Action::Group(GroupBy::Size),
    ),
    direct(("Não agrupar", "Don't group"), Action::Group(GroupBy::None)),
    direct(
        ("Coluna: dimensões", "Column: dimensions"),
        Action::MediaColumn(MediaCol::Dimensions),
    ),
    direct(
        ("Coluna: duração", "Column: duration"),
        Action::MediaColumn(MediaCol::Duration),
    ),
    direct(
        ("Coluna: data da foto", "Column: date taken"),
        Action::MediaColumn(MediaCol::Taken),
    ),
    direct(
        (
            "Comparar com o outro painel (painel duplo)",
            "Compare with the other pane (dual pane)",
        ),
        Action::Compare,
    ),
    direct(
        (
            "Salvar abas como espaço de trabalho…",
            "Save tabs as a workspace…",
        ),
        Action::SaveWorkspace,
    ),
    tab(("Propriedades", "Properties"), "alt-enter"),
    tab(("Abrir com…", "Open with…"), "shift-enter"),
    tab(
        ("Abrir no Terminal aqui", "Open in Terminal here"),
        "ctrl-`",
    ),
    tab(("Menu de contexto", "Context menu"), "shift-f10"),
    tab(("Editar caminho", "Edit path"), "ctrl-l"),
    tab(("Pasta acima", "Up one folder"), "alt-up"),
    tab(("Voltar", "Back"), "alt-left"),
    tab(("Avançar", "Forward"), "alt-right"),
    window(("Nova aba", "New tab"), "ctrl-t"),
    window(("Fechar aba", "Close tab"), "ctrl-w"),
    window(("Próxima aba", "Next tab"), "ctrl-tab"),
    window(("Aba anterior", "Previous tab"), "ctrl-shift-tab"),
    window(
        (
            "Mostrar ou esconder a barra lateral",
            "Show or hide the sidebar",
        ),
        "ctrl-b",
    ),
    window(
        (
            "Mostrar ou esconder itens ocultos",
            "Show or hide hidden items",
        ),
        "ctrl-h",
    ),
    window(
        ("Histórico de operações", "Operation history"),
        "ctrl-shift-h",
    ),
    window(("Densidade compacta", "Compact density"), "ctrl-1"),
    window(("Densidade padrão", "Standard density"), "ctrl-2"),
    window(("Densidade confortável", "Comfortable density"), "ctrl-3"),
    direct(
        ("Alternar grade e lista", "Toggle grid and list"),
        Action::ToggleGrid,
    ),
    direct(
        ("Ordenar por nome", "Sort by name"),
        Action::Sort(SortKey::Name),
    ),
    direct(
        ("Ordenar por tipo", "Sort by type"),
        Action::Sort(SortKey::Type),
    ),
    direct(
        ("Ordenar por tamanho", "Sort by size"),
        Action::Sort(SortKey::Size),
    ),
    direct(
        ("Ordenar por data de modificação", "Sort by date modified"),
        Action::Sort(SortKey::Modified),
    ),
    direct(("Sobre o fx", "About fx"), Action::About),
    direct(
        (
            "Ativar a busca no disco inteiro",
            "Turn on whole-disk search",
        ),
        Action::IndexService(true),
    ),
    direct(
        (
            "Desativar a busca no disco inteiro",
            "Turn off whole-disk search",
        ),
        Action::IndexService(false),
    ),
    direct(
        (
            "Tornar o fx o gerenciador padrão",
            "Make fx the default file manager",
        ),
        Action::DefaultManager(true),
    ),
    direct(
        (
            "Voltar ao Explorador como padrão",
            "Make Explorer the default again",
        ),
        Action::DefaultManager(false),
    ),
    direct(("Idioma: English", "Language: Português"), Action::Language),
    direct(
        (
            "Manter sempre neste dispositivo (nuvem)",
            "Always keep on this device (cloud)",
        ),
        Action::CloudPin(true),
    ),
    direct(
        ("Liberar espaço (nuvem)", "Free up space (cloud)"),
        Action::CloudPin(false),
    ),
    direct(
        ("Relatar a última falha", "Report the last crash"),
        Action::ReportCrash,
    ),
    direct(
        (
            "Relatórios de falha: ligar ou desligar",
            "Crash reports: turn on or off",
        ),
        Action::CrashReports,
    ),
    direct(("Atualizar o fx", "Update fx"), Action::Update),
    direct(
        (
            "Atualizações automáticas: ligar ou desligar",
            "Automatic updates: turn on or off",
        ),
        Action::UpdateChecks,
    ),
    window(("Como usar o fx", "How to use fx"), "f1"),
    direct(
        (
            "Confirmar antes de enviar para a Lixeira",
            "Confirm before sending to the Recycle Bin",
        ),
        Action::ConfirmRecycle,
    ),
    window(("Configurações", "Settings"), "ctrl-,"),
];

/// "ctrl-shift-n" → "Ctrl+Shift+N", "alt-up" → "Alt+↑", "space" → "Espaço".
pub fn shortcut_label(keys: &str) -> String {
    keys.split('-')
        .map(|part| match part {
            "ctrl" => "Ctrl".to_owned(),
            "shift" => "Shift".to_owned(),
            "alt" => "Alt".to_owned(),
            "space" => tr!("Espaço", "Space").to_owned(),
            "enter" => "Enter".to_owned(),
            "delete" => "Delete".to_owned(),
            "tab" => "Tab".to_owned(),
            "up" => "↑".to_owned(),
            "down" => "↓".to_owned(),
            "left" => "←".to_owned(),
            "right" => "→".to_owned(),
            other => other.to_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_unique_and_keys_parse() {
        let mut labels: Vec<&str> = COMMANDS.iter().map(|c| c.label()).collect();
        labels.sort();
        let before = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), before, "duplicate label");
        let mut keys = Vec::new();
        for c in COMMANDS {
            if let Action::Key(_, k) = c.action {
                assert!(gpui::Keystroke::parse(k).is_ok(), "{k}");
                keys.push(k);
            }
        }
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), n, "two commands share a shortcut");
    }

    #[test]
    fn shortcut_labels() {
        assert_eq!(shortcut_label("ctrl-shift-n"), "Ctrl+Shift+N");
        assert_eq!(shortcut_label("alt-up"), "Alt+↑");
        assert_eq!(shortcut_label("space"), "Espaço");
        assert_eq!(shortcut_label("shift-f10"), "Shift+F10");
        assert_eq!(shortcut_label("ctrl-`"), "Ctrl+`");
        assert_eq!(COMMANDS[0].shortcut().as_deref(), Some("Ctrl+F"));
        let grid = COMMANDS.iter().find(|c| c.action == Action::ToggleGrid);
        assert_eq!(grid.and_then(Command::shortcut), None);
    }

    #[test]
    fn palette_search_finds_commands() {
        let labels = COMMANDS.iter().map(|c| c.label());
        let first = fx_core::fuzzy::fuzzy_rank("nova p", labels)[0];
        assert_eq!(COMMANDS[first].label(), "Nova pasta");
        let labels = COMMANDS.iter().map(|c| c.label());
        let first = fx_core::fuzzy::fuzzy_rank("ocultos", labels)[0];
        assert_eq!(COMMANDS[first].label(), "Mostrar ou esconder itens ocultos");
    }
}

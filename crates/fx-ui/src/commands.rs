//! The command palette's commands (task T1.24). Most commands *are* their
//! shortcut: running one delivers the same keystroke to the window or the
//! active tab, so the palette and the keyboard can never disagree. A few have
//! no key and act directly.

use fx_core::SortKey;
#[allow(unused_imports)]
use fx_core::{tr, trf};

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
    direct(("Sobre o fx (versão)", "About fx (version)"), Action::About),
    direct(
        (
            "Ativar a busca no disco inteiro (serviço, pede administrador)",
            "Turn on whole-disk search (service, asks for administrator)",
        ),
        Action::IndexService(true),
    ),
    direct(
        (
            "Desativar a busca no disco inteiro (remove o serviço)",
            "Turn off whole-disk search (removes the service)",
        ),
        Action::IndexService(false),
    ),
    direct(
        (
            "Tornar o fx o gerenciador de arquivos padrão (pastas e Win+E)",
            "Make fx the default file manager (folders and Win+E)",
        ),
        Action::DefaultManager(true),
    ),
    direct(
        (
            "Voltar ao Explorador como gerenciador padrão",
            "Back to Explorer as the default file manager",
        ),
        Action::DefaultManager(false),
    ),
    direct(("Idioma: English", "Language: Português"), Action::Language),
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

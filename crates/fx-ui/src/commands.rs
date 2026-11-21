//! The command palette's commands (task T1.24). Most commands *are* their
//! shortcut: running one delivers the same keystroke to the window or the
//! active tab, so the palette and the keyboard can never disagree. A few have
//! no key and act directly.

use fx_core::SortKey;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub label: &'static str,
    pub action: Action,
}

impl Command {
    /// The shortcut as shown next to the command ("Ctrl+Shift+N").
    pub fn shortcut(&self) -> Option<String> {
        match self.action {
            Action::Key(_, keys) => Some(shortcut_label(keys)),
            _ => None,
        }
    }
}

const fn window(label: &'static str, keys: &'static str) -> Command {
    Command {
        label,
        action: Action::Key(Target::Window, keys),
    }
}

const fn tab(label: &'static str, keys: &'static str) -> Command {
    Command {
        label,
        action: Action::Key(Target::Tab, keys),
    }
}

const fn direct(label: &'static str, action: Action) -> Command {
    Command { label, action }
}

pub const COMMANDS: &[Command] = &[
    tab("Buscar no disco", "ctrl-f"),
    tab("Pré-visualizar (Quick Look)", "space"),
    tab("Nova pasta", "ctrl-shift-n"),
    tab("Renomear", "f2"),
    tab("Enviar para a Lixeira", "delete"),
    tab("Excluir permanentemente", "shift-delete"),
    tab("Copiar", "ctrl-c"),
    tab("Recortar", "ctrl-x"),
    tab("Colar", "ctrl-v"),
    tab("Copiar caminho", "ctrl-shift-c"),
    tab("Desfazer", "ctrl-z"),
    tab("Refazer", "ctrl-y"),
    tab("Selecionar tudo", "ctrl-a"),
    tab("Propriedades", "alt-enter"),
    tab("Abrir com…", "shift-enter"),
    tab("Abrir no Terminal aqui", "ctrl-`"),
    tab("Menu de contexto", "shift-f10"),
    tab("Editar caminho", "ctrl-l"),
    tab("Pasta acima", "alt-up"),
    tab("Voltar", "alt-left"),
    tab("Avançar", "alt-right"),
    window("Nova aba", "ctrl-t"),
    window("Fechar aba", "ctrl-w"),
    window("Próxima aba", "ctrl-tab"),
    window("Aba anterior", "ctrl-shift-tab"),
    window("Mostrar ou esconder a barra lateral", "ctrl-b"),
    window("Mostrar ou esconder itens ocultos", "ctrl-h"),
    window("Histórico de operações", "ctrl-shift-h"),
    window("Densidade compacta", "ctrl-1"),
    window("Densidade padrão", "ctrl-2"),
    window("Densidade confortável", "ctrl-3"),
    direct("Alternar grade e lista", Action::ToggleGrid),
    direct("Ordenar por nome", Action::Sort(SortKey::Name)),
    direct("Ordenar por tipo", Action::Sort(SortKey::Type)),
    direct("Ordenar por tamanho", Action::Sort(SortKey::Size)),
    direct(
        "Ordenar por data de modificação",
        Action::Sort(SortKey::Modified),
    ),
];

/// "ctrl-shift-n" → "Ctrl+Shift+N", "alt-up" → "Alt+↑", "space" → "Espaço".
pub fn shortcut_label(keys: &str) -> String {
    keys.split('-')
        .map(|part| match part {
            "ctrl" => "Ctrl".to_owned(),
            "shift" => "Shift".to_owned(),
            "alt" => "Alt".to_owned(),
            "space" => "Espaço".to_owned(),
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
        let mut labels: Vec<&str> = COMMANDS.iter().map(|c| c.label).collect();
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
        let labels = COMMANDS.iter().map(|c| c.label);
        let first = fx_core::fuzzy::fuzzy_rank("nova p", labels)[0];
        assert_eq!(COMMANDS[first].label, "Nova pasta");
        let labels = COMMANDS.iter().map(|c| c.label);
        let first = fx_core::fuzzy::fuzzy_rank("ocultos", labels)[0];
        assert_eq!(COMMANDS[first].label, "Mostrar ou esconder itens ocultos");
    }
}

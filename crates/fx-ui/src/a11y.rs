//! Accessibility tree of a window (task T1.10, D-008): window → path bar →
//! file list (with the total number of items) → the rows on screen.
//!
//! The list is virtualized, so only the visible rows exist as nodes; each one
//! carries its position and the list carries the total, which screen readers
//! announce as "x de N". Built by the active tab every time it draws its rows
//! (a few dozen nodes, microseconds) and sent to the adapter only when a
//! screen reader is listening.

use std::cell::RefCell;
use std::rc::Rc;

use accesskit::{Action, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use fx_shell::{A11y, LatestTree};

const WINDOW: NodeId = NodeId(1);
const PATH: NodeId = NodeId(2);
const LIST: NodeId = NodeId(3);
/// Row nodes are `ROW_BASE + entry index`.
const ROW_BASE: u64 = 1 << 32;

/// The adapter of a window and its latest tree, shared by its tabs.
#[derive(Clone)]
pub struct A11yHandle {
    adapter: Rc<RefCell<A11y>>,
    latest: LatestTree,
}

/// One visible row.
pub struct Row {
    pub entry: u32,
    /// Position in the whole (filtered) list, from 0.
    pub position: usize,
    pub name: String,
    pub description: String,
    pub selected: bool,
}

/// What the tree shows.
pub struct Snapshot {
    pub title: String,
    pub path: String,
    /// Text of the path field while it is being edited.
    pub editing: Option<String>,
    pub total: usize,
    pub rows: Vec<Row>,
    /// Entry with the keyboard cursor.
    pub cursor: Option<u32>,
}

impl A11yHandle {
    pub fn new(adapter: A11y, latest: LatestTree) -> Self {
        Self {
            adapter: Rc::new(RefCell::new(adapter)),
            latest,
        }
    }

    pub fn publish(&self, snapshot: &Snapshot) {
        let tree = build(snapshot);
        if let Ok(mut latest) = self.latest.try_borrow_mut() {
            *latest = Some(tree.clone());
        }
        if let Ok(mut adapter) = self.adapter.try_borrow_mut() {
            adapter.update(|| tree);
        }
    }
}

/// Entry index of a row node, if `node` is one.
pub fn row_entry(node: NodeId) -> Option<u32> {
    node.0
        .checked_sub(ROW_BASE)
        .and_then(|i| u32::try_from(i).ok())
}

pub fn build(s: &Snapshot) -> TreeUpdate {
    let mut window = Node::new(Role::Window);
    window.set_label(s.title.clone());
    window.set_children(vec![PATH, LIST]);

    let path = match &s.editing {
        Some(text) => {
            let mut n = Node::new(Role::TextInput);
            n.set_label("Caminho");
            n.set_value(text.clone());
            n
        }
        None => {
            // A Label's text is its value (that is what UIA exposes as Name).
            let mut n = Node::new(Role::Label);
            let text = format!("Caminho: {}", s.path);
            n.set_label(text.clone());
            n.set_value(text);
            n
        }
    };

    let mut list = Node::new(Role::ListBox);
    list.set_label("Arquivos");
    list.set_description(match s.total {
        1 => "1 item".to_owned(),
        n => format!("{n} itens"),
    });
    list.set_size_of_set(s.total);
    list.set_children(
        s.rows
            .iter()
            .map(|r| NodeId(ROW_BASE + u64::from(r.entry)))
            .collect::<Vec<_>>(),
    );

    let mut nodes = Vec::with_capacity(s.rows.len() + 3);
    nodes.push((WINDOW, window));
    nodes.push((PATH, path));
    nodes.push((LIST, list));
    let mut focus = if s.editing.is_some() { PATH } else { LIST };
    for r in &s.rows {
        let id = NodeId(ROW_BASE + u64::from(r.entry));
        let mut n = Node::new(Role::ListBoxOption);
        n.set_label(r.name.clone());
        n.set_description(r.description.clone());
        n.set_position_in_set(r.position);
        n.set_selected(r.selected);
        n.add_action(Action::Focus);
        n.add_action(Action::Click);
        if s.editing.is_none() && s.cursor == Some(r.entry) {
            focus = id;
        }
        nodes.push((id, n));
    }
    let mut info = TreeInfo::new(WINDOW);
    info.toolkit_name = Some("fx".into());
    info.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
    TreeUpdate {
        nodes,
        tree: Some(info),
        tree_id: TreeId::ROOT,
        focus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(cursor: Option<u32>, editing: Option<String>) -> Snapshot {
        Snapshot {
            title: "Fotos — fx".into(),
            path: r"C:\Fotos".into(),
            editing,
            total: 1000,
            rows: (10..13)
                .map(|e| Row {
                    entry: e,
                    position: e as usize + 90,
                    name: format!("foto-{e}.jpg"),
                    description: "13 KB".into(),
                    selected: e == 11,
                })
                .collect(),
            cursor,
        }
    }

    #[test]
    fn tree_has_window_path_list_and_visible_rows() {
        let t = build(&snapshot(Some(11), None));
        assert_eq!(t.nodes.len(), 6);
        assert_eq!(t.tree.as_ref().map(|i| i.root), Some(WINDOW));
        let list = &t.nodes.iter().find(|(id, _)| *id == LIST).unwrap().1;
        assert_eq!(list.size_of_set(), Some(1000));
        assert_eq!(list.children().len(), 3);
        let (id, row) = &t.nodes[4];
        assert_eq!(row_entry(*id), Some(11));
        assert_eq!(row.label(), Some("foto-11.jpg"));
        assert_eq!(row.position_in_set(), Some(101));
        assert_eq!(row.is_selected(), Some(true));
        assert_eq!(t.focus, *id, "focus on the cursor row");
    }

    #[test]
    fn focus_falls_back_to_the_list_or_the_path_field() {
        assert_eq!(
            build(&snapshot(Some(999), None)).focus,
            LIST,
            "cursor off screen"
        );
        assert_eq!(build(&snapshot(None, None)).focus, LIST);
        let editing = build(&snapshot(Some(11), Some(r"C:\Fo".into())));
        assert_eq!(editing.focus, PATH);
        assert_eq!(row_entry(PATH), None);
    }
}

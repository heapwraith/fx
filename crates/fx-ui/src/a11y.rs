//! Accessibility tree of a window (task T1.10, D-008): window → path bar →
//! file list (with the total number of items) → the rows on screen, and,
//! over them, the dialog or panel that has the keyboard (review, D-066: the
//! palette, Settings, the dialogs and the panels used not to exist for a
//! screen reader).
//!
//! The list is virtualized, so only the visible rows exist as nodes; each one
//! carries its position and the list carries the total, which screen readers
//! announce as "x de N". Built by the active tab every time it draws its rows
//! (a few dozen nodes, microseconds) and sent to the adapter only when a
//! screen reader is listening. Overlays come in three layers (the tab's
//! panels, the window's dialogs, a confirmation on top); the highest one
//! shown is the modal dialog in the tree, with the focus on its active item.

use fx_core::{tr, trf};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use accesskit::{Action, Node, NodeId, Role, Toggled, TreeId, TreeInfo, TreeUpdate};
use fx_shell::{A11y, LatestTree};

const WINDOW: NodeId = NodeId(1);
const PATH: NodeId = NodeId(2);
const LIST: NodeId = NodeId(3);
const OVERLAY: NodeId = NodeId(4);
const OVERLAY_LIST: NodeId = NodeId(5);
/// Overlay item nodes are `ITEM_BASE + index`.
const ITEM_BASE: u64 = 1 << 16;
/// Row nodes are `ROW_BASE + entry index`.
const ROW_BASE: u64 = 1 << 32;

/// Where an overlay sits; the highest one shown is the one in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// A panel of the tab: search, Quick Look, batch rename, terminal.
    Tab = 0,
    /// A dialog of the window: conflict, Settings, guide, palette, history.
    Window = 1,
    /// A confirmation (`window.prompt`), over everything.
    Prompt = 2,
}

/// A dialog or panel over the list, as a screen reader should read it.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub title: String,
    pub description: Option<String>,
    pub items: Vec<Item>,
    /// The item with the keyboard; the dialog itself when `None`.
    pub focus: Option<usize>,
    /// When the choices are a window of a longer list (search results):
    /// the position of the first one and the total, for "x de N".
    pub positions: Option<(usize, usize)>,
}

/// One thing in an overlay.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Button(String),
    /// A choice in the overlay's list ("x de N").
    Choice {
        label: String,
        description: Option<String>,
        selected: bool,
    },
    Switch {
        label: String,
        on: bool,
    },
    /// One option of a choice (language, density).
    Radio {
        label: String,
        checked: bool,
    },
    Input {
        label: String,
        value: String,
    },
    Text(String),
}

type PromptClick = Box<dyn Fn(usize, &mut gpui::App)>;

struct Inner {
    adapter: RefCell<A11y>,
    latest: LatestTree,
    /// The list, as the active tab last drew it.
    list: RefCell<Option<Snapshot>>,
    overlays: RefCell<[Option<Overlay>; 3]>,
    /// Answers the confirmation on top (its buttons, by index).
    prompt_click: RefCell<Option<PromptClick>>,
}

/// The adapter of a window and what its tree shows, shared by its tabs.
#[derive(Clone)]
pub struct A11yHandle(Rc<Inner>);

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

thread_local! {
    /// Each window's handle, for the confirmations (built by GPUI, which
    /// only hands them the window).
    static WINDOWS: RefCell<Vec<(gpui::WindowId, Weak<Inner>)>> = const { RefCell::new(Vec::new()) };
}

impl A11yHandle {
    pub fn new(adapter: A11y, latest: LatestTree) -> Self {
        Self(Rc::new(Inner {
            adapter: RefCell::new(adapter),
            latest,
            list: RefCell::new(None),
            overlays: RefCell::new([None, None, None]),
            prompt_click: RefCell::new(None),
        }))
    }

    /// Makes this the handle of `window` (for its confirmations).
    pub fn register(&self, window: gpui::WindowId) {
        WINDOWS.with(|w| {
            let mut w = w.borrow_mut();
            w.retain(|(_, h)| h.strong_count() > 0);
            w.push((window, Rc::downgrade(&self.0)));
        });
    }

    /// The handle of `window`, if it has one.
    pub fn of_window(window: gpui::WindowId) -> Option<Self> {
        WINDOWS.with(|w| {
            w.borrow()
                .iter()
                .find(|(id, _)| *id == window)
                .and_then(|(_, h)| h.upgrade())
                .map(Self)
        })
    }

    /// The list as the active tab draws it.
    pub fn publish(&self, snapshot: Snapshot) {
        *self.0.list.borrow_mut() = Some(snapshot);
        self.send();
    }

    /// Shows (or, with `None`, removes) the overlay of `layer`. Sends only
    /// when it changed, so calling it on every frame costs a comparison.
    pub fn set_overlay(&self, layer: Layer, overlay: Option<Overlay>) {
        {
            let mut all = self.0.overlays.borrow_mut();
            let slot = &mut all[layer as usize];
            if *slot == overlay {
                return;
            }
            *slot = overlay;
        }
        self.send();
    }

    /// The layer of the overlay in the tree, if any.
    pub fn top_layer(&self) -> Option<Layer> {
        let all = self.0.overlays.borrow();
        [Layer::Prompt, Layer::Window, Layer::Tab]
            .into_iter()
            .find(|l| all[*l as usize].is_some())
    }

    /// How a screen reader's click answers the confirmation on top.
    pub fn set_prompt_click(&self, click: Option<PromptClick>) {
        *self.0.prompt_click.borrow_mut() = click;
    }

    /// A screen reader clicked button `index` of the confirmation.
    pub fn click_prompt(&self, index: usize, cx: &mut gpui::App) {
        if let Some(click) = self.0.prompt_click.borrow().as_ref() {
            click(index, cx);
        }
    }

    fn send(&self) {
        let list = self.0.list.borrow();
        let Some(list) = list.as_ref() else {
            return;
        };
        let overlays = self.0.overlays.borrow();
        let top = overlays.iter().rev().flatten().next();
        let tree = build(list, top);
        if let Ok(mut latest) = self.0.latest.try_borrow_mut() {
            *latest = Some(tree.clone());
        }
        if let Ok(mut adapter) = self.0.adapter.try_borrow_mut() {
            adapter.update(|| tree);
        }
    }
}

/// Item index of an overlay node, if `node` is one.
pub fn overlay_item(node: NodeId) -> Option<usize> {
    node.0
        .checked_sub(ITEM_BASE)
        .filter(|&k| k < ROW_BASE - ITEM_BASE)
        .and_then(|k| usize::try_from(k).ok())
}

/// Entry index of a row node, if `node` is one.
pub fn row_entry(node: NodeId) -> Option<u32> {
    node.0
        .checked_sub(ROW_BASE)
        .and_then(|i| u32::try_from(i).ok())
}

pub fn build(s: &Snapshot, overlay: Option<&Overlay>) -> TreeUpdate {
    let mut window = Node::new(Role::Window);
    window.set_label(s.title.clone());
    let mut children = vec![PATH, LIST];
    if overlay.is_some() {
        children.push(OVERLAY);
    }
    window.set_children(children);

    let path = match &s.editing {
        Some(text) => {
            let mut n = Node::new(Role::TextInput);
            n.set_label(tr!("Caminho", "Path"));
            n.set_value(text.clone());
            n
        }
        None => {
            // A Label's text is its value (that is what UIA exposes as Name).
            let mut n = Node::new(Role::Label);
            let text = trf!("Caminho: {}", "Path: {}", s.path);
            n.set_label(text.clone());
            n.set_value(text);
            n
        }
    };

    let mut list = Node::new(Role::ListBox);
    list.set_label(tr!("Arquivos", "Files"));
    list.set_description(fx_core::format::items(s.total));
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
    if let Some(o) = overlay {
        focus = overlay_nodes(o, &mut nodes);
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

/// The overlay's nodes (a modal dialog, its items, the list of its
/// choices); returns the node with the focus.
fn overlay_nodes(o: &Overlay, nodes: &mut Vec<(NodeId, Node)>) -> NodeId {
    let id = |k: usize| NodeId(ITEM_BASE + k as u64);
    let mut dialog = Node::new(Role::Dialog);
    dialog.set_modal();
    dialog.set_label(o.title.clone());
    if let Some(d) = &o.description {
        dialog.set_description(d.clone());
    }
    let choices: Vec<usize> = (0..o.items.len())
        .filter(|&k| matches!(o.items[k], Item::Choice { .. }))
        .collect();
    let mut children: Vec<NodeId> = (0..o.items.len())
        .filter(|k| !choices.contains(k))
        .map(id)
        .collect();
    if !choices.is_empty() {
        children.push(OVERLAY_LIST);
        let mut list = Node::new(Role::ListBox);
        list.set_label(o.title.clone());
        list.set_size_of_set(o.positions.map_or(choices.len(), |(_, total)| total));
        list.set_children(choices.iter().map(|&k| id(k)).collect::<Vec<_>>());
        nodes.push((OVERLAY_LIST, list));
    }
    dialog.set_children(children);
    nodes.push((OVERLAY, dialog));
    for (k, item) in o.items.iter().enumerate() {
        let node = match item {
            Item::Button(label) => {
                let mut n = Node::new(Role::Button);
                n.set_label(label.clone());
                n.add_action(Action::Click);
                n.add_action(Action::Focus);
                n
            }
            Item::Choice {
                label,
                description,
                selected,
            } => {
                let mut n = Node::new(Role::ListBoxOption);
                n.set_label(label.clone());
                if let Some(d) = description {
                    n.set_description(d.clone());
                }
                let local = choices.iter().position(|&c| c == k).unwrap_or(0);
                n.set_position_in_set(o.positions.map_or(0, |(first, _)| first) + local);
                n.set_selected(*selected);
                n.add_action(Action::Click);
                n.add_action(Action::Focus);
                n
            }
            Item::Switch { label, on } => {
                let mut n = Node::new(Role::Switch);
                n.set_label(label.clone());
                n.set_toggled(if *on { Toggled::True } else { Toggled::False });
                n.add_action(Action::Click);
                n
            }
            Item::Radio { label, checked } => {
                let mut n = Node::new(Role::RadioButton);
                n.set_label(label.clone());
                n.set_toggled(if *checked {
                    Toggled::True
                } else {
                    Toggled::False
                });
                n.add_action(Action::Click);
                n
            }
            Item::Input { label, value } => {
                let mut n = Node::new(Role::TextInput);
                n.set_label(label.clone());
                n.set_value(value.clone());
                n
            }
            Item::Text(text) => {
                let mut n = Node::new(Role::Label);
                n.set_label(text.clone());
                n.set_value(text.clone());
                n
            }
        };
        nodes.push((id(k), node));
    }
    o.focus.filter(|&k| k < o.items.len()).map_or(OVERLAY, id)
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
        let t = build(&snapshot(Some(11), None), None);
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
            build(&snapshot(Some(999), None), None).focus,
            LIST,
            "cursor off screen"
        );
        assert_eq!(build(&snapshot(None, None), None).focus, LIST);
        let editing = build(&snapshot(Some(11), Some(r"C:\Fo".into())), None);
        assert_eq!(editing.focus, PATH);
        assert_eq!(row_entry(PATH), None);
    }

    #[test]
    fn a_dialog_takes_the_focus_and_lists_its_choices() {
        let o = Overlay {
            title: "Enviar para a Lixeira?".into(),
            description: Some("3 itens vão para a Lixeira.".into()),
            items: vec![
                Item::Input {
                    label: "Buscar".into(),
                    value: "rel".into(),
                },
                Item::Choice {
                    label: "relatório.pdf".into(),
                    description: None,
                    selected: true,
                },
                Item::Choice {
                    label: "relógio.txt".into(),
                    description: None,
                    selected: false,
                },
                Item::Button("Cancelar".into()),
            ],
            focus: Some(2),
            positions: None,
        };
        let t = build(&snapshot(Some(11), None), Some(&o));
        let node = |id: NodeId| &t.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
        assert!(node(WINDOW).children().contains(&OVERLAY));
        assert!(node(OVERLAY).is_modal());
        assert_eq!(node(OVERLAY).label(), Some("Enviar para a Lixeira?"));
        assert_eq!(node(OVERLAY_LIST).size_of_set(), Some(2));
        let focused = node(t.focus);
        assert_eq!(focused.label(), Some("relógio.txt"));
        assert_eq!(focused.position_in_set(), Some(1));
        assert_eq!(overlay_item(t.focus), Some(2));
        assert_eq!(
            overlay_item(NodeId(ROW_BASE + 5)),
            None,
            "rows are not items"
        );
        // No focus given: the dialog itself.
        let o = Overlay { focus: None, ..o };
        assert_eq!(build(&snapshot(None, None), Some(&o)).focus, OVERLAY);
    }
}

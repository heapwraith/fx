//! Drag and drop (task T1.15). Dropping files from any app (or another fx
//! window) on the list copies or moves them into the folder shown, or into a
//! folder row, through the operations queue (conflicts asked, undo recorded).
//! The operation follows the Explorer: same volume moves, another volume
//! copies, Shift moves, Ctrl copies. Dragging rows out hands the shell's own
//! data object to the target (Explorer, desktop, a browser upload...), which
//! performs the operation itself.

use fx_core::{tr, trf};
use std::path::PathBuf;

use fx_ops::{Job, OpKind, Resolution, drop_operation};
use gpui::{Context, MouseMoveEvent, Pixels, Point, Window};

use super::FolderView;

/// The pointer must travel this far with the button down to start a drag.
pub(super) const DRAG_THRESHOLD: f32 = 6.0;

impl FolderView {
    /// A row was pressed: a drag may start from here.
    pub(super) fn press_row(&mut self, at: Point<Pixels>, entry: u32) {
        self.press = Some((at, entry));
    }

    /// The button went up on row `entry` without a drag: a plain click on a
    /// row of a multiple selection selects only that row (like the Explorer).
    pub(super) fn release_on(&mut self, entry: u32) {
        if let Some((_, pressed)) = self.press.take()
            && pressed == entry
            && self.selection.len() > 1
            && self.renaming.is_none()
        {
            self.selection.select_only(entry);
        }
    }

    /// Starts dragging the selection out once the pointer moved far enough.
    pub(super) fn maybe_drag(
        &mut self,
        ev: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((start, _)) = self.press else { return };
        if !ev.dragging() {
            self.press = None;
            return;
        }
        let d = ev.position - start;
        if f32::from(d.x).hypot(f32::from(d.y)) < DRAG_THRESHOLD {
            return;
        }
        self.press = None;
        let paths = self.targets();
        let Some(hwnd) = fx_shell::window_id(window) else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        // The OLE drag runs its own message loop: start it from a task, where
        // no app state is borrowed, so the window keeps drawing meanwhile.
        cx.spawn(async move |this, cx| {
            let result = fx_shell::drag_files(hwnd, &paths);
            let _ = this.update(cx, |view, cx| {
                view.notice = match result {
                    Ok(fx_shell::DragResult::Moved) => {
                        Some(tr!("movido pelo destino", "moved by the target").into())
                    }
                    Ok(fx_shell::DragResult::Copied) => {
                        Some(tr!("copiado pelo destino", "copied by the target").into())
                    }
                    Ok(fx_shell::DragResult::Linked) => Some(
                        tr!(
                            "atalho criado pelo destino",
                            "shortcut created by the target"
                        )
                        .into(),
                    ),
                    Ok(fx_shell::DragResult::None) => None,
                    Err(e) => Some(trf!(
                        "não foi possível arrastar: {e}",
                        "could not drag: {e}"
                    )),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Files dropped on folder `dest` (the folder shown or a folder row).
    pub(super) fn drop_into(
        &mut self,
        paths: Vec<PathBuf>,
        dest: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (shift, ctrl) = fx_shell::drop_keys();
        let kind = drop_operation(&paths, &dest, shift, ctrl);
        // Dropping items on the folder they are in (or a folder on itself) is
        // not an operation.
        let paths: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| *p != dest && !(kind == OpKind::Move && p.parent() == Some(dest.as_path())))
            .collect();
        if paths.is_empty() {
            return;
        }
        let job = match kind {
            OpKind::Move => Job::move_to(paths, dest),
            _ => Job::copy(paths, dest),
        };
        self.submit_job(job.on_conflict(Resolution::Ask), window, cx);
    }
}

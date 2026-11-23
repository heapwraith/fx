//! A thin overlay scrollbar for any scrolling `div` (the list has its own,
//! tied to its virtual list): it shows where you are, only when there is
//! something to scroll, and its thumb can be dragged. Read at paint time,
//! so it follows the wheel in the same frame.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Bounds, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    ScrollHandle, Styled, canvas, fill, point, px, size,
};

use crate::theme::{RADIUS, Theme, rgba};

/// Width of the strip the thumb runs in.
pub const WIDTH: f32 = 10.0;
/// The thumb never gets shorter than this.
const MIN_THUMB: f32 = 24.0;

/// A scroll position and the drag of its thumb; kept by the view, cloned
/// into the scrolling element and its bar.
#[derive(Clone, Default)]
pub struct Scroll {
    pub handle: ScrollHandle,
    /// While dragging: where in the thumb the pointer took it.
    drag: Rc<Cell<Option<f32>>>,
}

impl Scroll {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Where the thumb is inside `track`, or `None` when nothing scrolls.
fn thumb(handle: &ScrollHandle, track: Bounds<Pixels>) -> Option<(Bounds<Pixels>, f32, f32)> {
    let view_h = f32::from(handle.bounds().size.height);
    let max = f32::from(handle.max_offset().height);
    if view_h <= 0.0 || max < 1.0 {
        return None;
    }
    let track_h = f32::from(track.size.height);
    let thumb_h = (track_h * view_h / (view_h + max))
        .max(MIN_THUMB)
        .min(track_h);
    let at = (-f32::from(handle.offset().y) / max).clamp(0.0, 1.0);
    let top = f32::from(track.origin.y) + (track_h - thumb_h) * at;
    let rect = Bounds::new(
        point(track.origin.x + px(2.0), px(top)),
        size(track.size.width - px(4.0), px(thumb_h)),
    );
    Some((rect, track_h - thumb_h, max))
}

/// The bar for `scroll`, laid over the right edge of its (relative) parent.
pub fn vertical(scroll: &Scroll, t: &Theme) -> impl IntoElement {
    let handle = scroll.handle.clone();
    let drag = scroll.drag.clone();
    let (idle, hover) = (t.thumb, t.thumb_hover);
    canvas(
        |_, _, _| (),
        move |track, _, window, _| {
            let Some((rect, room, max)) = thumb(&handle, track) else {
                return;
            };
            let active = drag.get().is_some() || rect.contains(&window.mouse_position());
            window.paint_quad(
                fill(rect, rgba(if active { hover } else { idle })).corner_radii(px(RADIUS)),
            );
            window.on_mouse_event({
                let drag = drag.clone();
                move |ev: &MouseDownEvent, phase, window, _| {
                    if phase.bubble()
                        && ev.button == MouseButton::Left
                        && rect.contains(&ev.position)
                    {
                        drag.set(Some(f32::from(ev.position.y - rect.origin.y)));
                        window.refresh();
                    }
                }
            });
            window.on_mouse_event({
                let (drag, handle) = (drag.clone(), handle.clone());
                move |ev: &MouseMoveEvent, _, window, _| {
                    let Some(grab) = drag.get() else {
                        return;
                    };
                    if !ev.dragging() {
                        drag.set(None);
                        return;
                    }
                    let top = f32::from(ev.position.y - track.origin.y) - grab;
                    let at = if room > 0.0 {
                        (top / room).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let x = handle.offset().x;
                    handle.set_offset(point(x, px(-at * max)));
                    window.refresh();
                }
            });
            window.on_mouse_event({
                let drag = drag.clone();
                move |_: &MouseUpEvent, _, window, _| {
                    if drag.take().is_some() {
                        window.refresh();
                    }
                }
            });
        },
    )
    .absolute()
    .top_0()
    .bottom_0()
    .right_0()
    .w(px(WIDTH))
}

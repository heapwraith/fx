//! Tooltips for the buttons that are only an icon (review of the interface,
//! D-067): the action and its shortcut, after the usual hover delay (GPUI's).
//! An icon alone says little to someone new, and nothing to a screen reader.

use gpui::{AnyView, App, Context, IntoElement, Render, SharedString, Window, div, prelude::*, px};

use crate::theme::{CAPTION_SIZE, RADIUS, Theme, rgba, space};

struct Tip {
    text: SharedString,
    keys: Option<SharedString>,
    theme: Theme,
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div()
            .px(px(space(2)))
            .py(px(space(1)))
            .flex()
            .gap(px(space(2)))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_md()
            .text_size(px(CAPTION_SIZE))
            .text_color(rgba(t.text))
            .child(self.text.clone())
            .when_some(self.keys.clone(), |d, k| {
                d.child(div().text_color(rgba(t.text_secondary)).child(k))
            })
    }
}

/// A tooltip builder: `text` and, if any, the shortcut.
pub fn tip(
    text: &'static str,
    keys: Option<&'static str>,
    theme: Theme,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_, cx| {
        cx.new(|_| Tip {
            text: text.into(),
            keys: keys.map(SharedString::from),
            theme,
        })
        .into()
    }
}

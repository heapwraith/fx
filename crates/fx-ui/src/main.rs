//! fx: the application.
//!
//! T0.1 (docs/phases/PHASE-0.md): a native window with a text and a button that
//! changes the text when clicked, proving GPUI runs on Windows.
#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

use gpui::{
    App, Application, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

/// Text shown above the button after `clicks` clicks.
fn label_for(clicks: u32) -> SharedString {
    match clicks {
        0 => "fx: GPUI on Windows. Click the button.".into(),
        1 => "Clicked once.".into(),
        n => format!("Clicked {n} times.").into(),
    }
}

struct Hello {
    clicks: u32,
}

impl Render for Hello {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .size_full()
            .justify_center()
            .items_center()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1a1a1a))
            .text_xl()
            .child(label_for(self.clicks))
            .child(
                div()
                    .id("click-me")
                    .px_4()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(0x2563eb))
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(0x1d4ed8)))
                    .active(|this| this.opacity(0.8))
                    .child("Click me")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clicks = this.clicks.saturating_add(1);
                        cx.notify();
                    })),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(480.0), px(320.0)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Hello { clicks: 0 }),
        );
        if let Err(err) = opened {
            eprintln!("fx: failed to open window: {err:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::label_for;

    #[test]
    fn label_changes_with_clicks() {
        assert_eq!(label_for(0), "fx: GPUI on Windows. Click the button.");
        assert_eq!(label_for(1), "Clicked once.");
        assert_eq!(label_for(2), "Clicked 2 times.");
        assert_ne!(label_for(0), label_for(1));
    }
}

//! fx's own confirmation dialog (found in the final QA): GPUI prompts used
//! the native Windows message box, with an English title ("Critical") and a
//! look unlike every other fx dialog. This renderer draws them with the fx
//! tokens, in the interface language. Enter picks the first button, Esc the
//! last (the cancel one, by fx's convention).

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, PromptButton, PromptHandle,
    PromptLevel, PromptResponse, RenderablePromptHandle, Window, div, prelude::*, px,
};

use crate::a11y::{A11yHandle, Item, Layer, Overlay};
use crate::theme::{BODY_SIZE, CAPTION_SIZE, Mode, RADIUS, Theme, rgba, space};

/// Installs the renderer (once, at startup).
pub fn install(cx: &mut App) {
    cx.set_prompt_builder(build);
}

fn build(
    level: PromptLevel,
    message: &str,
    detail: Option<&str>,
    actions: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let theme = Theme::current(Mode::from_appearance(window.appearance()));
    let labels: Vec<String> = actions.iter().map(|a| a.label().to_string()).collect();
    let a11y = A11yHandle::of_window(window.window_handle().window_id());
    let view = cx.new(|cx| FxPrompt {
        opened: std::time::Instant::now(),
        level,
        message: message.to_owned(),
        detail: detail.map(str::to_owned),
        actions: labels.clone(),
        focus: cx.focus_handle(),
        theme,
        a11y: a11y.clone(),
    });
    // Read by screen readers as a modal dialog, with the focus on the first
    // button (what Enter does); their click answers it (review, D-066).
    if let Some(h) = &a11y {
        h.set_overlay(Layer::Prompt, Some(overlay(message, detail, &labels)));
        let weak = view.downgrade();
        h.set_prompt_click(Some(Box::new(move |ix, cx| {
            let _ = weak.update(cx, |_, cx| cx.emit(PromptResponse(ix)));
        })));
    }
    handle.with_view(view, window, cx)
}

/// The dialog as a screen reader reads it.
fn overlay(message: &str, detail: Option<&str>, labels: &[String]) -> Overlay {
    let keys = match labels {
        [] => String::new(),
        // Key names: the same in both languages.
        [only] => format!("Enter: {only}."),
        [first, .., last] => format!("Enter: {first}. Esc: {last}."),
    };
    let description = match detail {
        Some(d) => format!("{d} {keys}"),
        None => keys,
    };
    Overlay {
        title: message.to_owned(),
        description: Some(description.trim().to_owned()),
        items: labels.iter().cloned().map(Item::Button).collect(),
        focus: (!labels.is_empty()).then_some(0),
        positions: None,
    }
}

struct FxPrompt {
    /// When the dialog appeared: an Enter right after (the press that opened
    /// it, or its bounce) must not answer it.
    opened: std::time::Instant,
    level: PromptLevel,
    message: String,
    detail: Option<String>,
    actions: Vec<String>,
    focus: FocusHandle,
    theme: Theme,
    /// The window's accessibility tree, cleared when the dialog goes.
    a11y: Option<A11yHandle>,
}

impl Drop for FxPrompt {
    fn drop(&mut self) {
        if let Some(h) = &self.a11y {
            h.set_overlay(Layer::Prompt, None);
            h.set_prompt_click(None);
        }
    }
}

impl EventEmitter<PromptResponse> for FxPrompt {}

impl Focusable for FxPrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Ignore Enter for this long after the dialog appears (found in review: a
/// held Enter in the palette confirmed "Excluir permanentemente" unseen).
const ENTER_GUARD: std::time::Duration = std::time::Duration::from_millis(300);

/// Which button a key picks: Enter the first, Esc the last. A held key
/// (auto-repeat) never answers, and Enter only after `ENTER_GUARD`.
fn answer_for(
    key: &str,
    buttons: usize,
    held: bool,
    since_open: std::time::Duration,
) -> Option<usize> {
    if held || buttons == 0 {
        return None;
    }
    match key {
        "enter" if since_open >= ENTER_GUARD => Some(0),
        "escape" => Some(buttons - 1),
        _ => None,
    }
}

impl FxPrompt {
    fn on_key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = answer_for(
            e.keystroke.key.as_str(),
            self.actions.len(),
            e.is_held,
            self.opened.elapsed(),
        ) {
            cx.emit(PromptResponse(ix));
        }
        cx.stop_propagation();
    }
}

impl Render for FxPrompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let danger = matches!(self.level, PromptLevel::Critical);
        let on_accent = if t.mode == Mode::Dark {
            0x000000FF
        } else {
            0xFFFFFFFF
        };
        let buttons = self.actions.iter().enumerate().map(|(ix, label)| {
            let primary = ix == 0;
            div()
                .id(ix)
                .h(px(32.0))
                .px(px(space(4)))
                .flex()
                .items_center()
                .rounded(px(RADIUS))
                .cursor_pointer()
                .when(primary, |d| {
                    d.bg(rgba(if danger { 0xC42B1CFF } else { t.accent }))
                        .text_color(rgba(if danger { 0xFFFFFFFF } else { on_accent }))
                })
                .when(!primary, |d| {
                    d.border_1()
                        .border_color(rgba(t.divider))
                        .bg(rgba(t.layer))
                        .hover(|s| s.bg(rgba(t.hover)))
                })
                .child(label.clone())
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(PromptResponse(ix))))
        });
        let card = div()
            .w(px(420.0))
            .p(px(space(5)))
            .flex()
            .flex_col()
            .gap(px(space(3)))
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .text_size(px(BODY_SIZE))
            .text_color(rgba(t.text))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(self.message.clone()),
            )
            .children(
                self.detail
                    .clone()
                    .map(|d| div().text_color(rgba(t.text_secondary)).child(d)),
            )
            .child(
                div()
                    .mt(px(space(1)))
                    .flex()
                    .justify_end()
                    .gap(px(space(2)))
                    .children(buttons),
            )
            .child(
                div()
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .child(fx_core::tr!(
                        "Enter confirma · Esc cancela",
                        "Enter confirms · Esc cancels"
                    )),
            );
        div()
            .id("fx-prompt")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            // Modal: nothing behind it gets the mouse (review: clicks reached
            // the list and took the keyboard focus away).
            .occlude()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(crate::theme::scrim(&t)))
            .child(card)
    }
}

#[cfg(test)]
mod tests {
    use super::{ENTER_GUARD, answer_for};
    use std::time::Duration;

    #[test]
    fn enter_confirms_and_escape_cancels() {
        let later = ENTER_GUARD + Duration::from_millis(1);
        assert_eq!(answer_for("enter", 2, false, later), Some(0));
        assert_eq!(answer_for("escape", 2, false, later), Some(1));
        assert_eq!(answer_for("x", 2, false, later), None);
        assert_eq!(answer_for("enter", 0, false, later), None);
    }

    #[test]
    fn a_held_or_early_enter_never_confirms() {
        let later = ENTER_GUARD + Duration::from_millis(1);
        assert_eq!(answer_for("enter", 2, true, later), None, "auto-repeat");
        assert_eq!(
            answer_for("enter", 2, false, Duration::from_millis(50)),
            None,
            "the opening press"
        );
        assert_eq!(
            answer_for("escape", 2, false, Duration::ZERO),
            Some(1),
            "Esc always cancels"
        );
        assert_eq!(answer_for("escape", 2, true, later), None);
    }
}

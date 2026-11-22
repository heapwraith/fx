//! fx's own confirmation dialog (found in the final QA): GPUI prompts used
//! the native Windows message box, with an English title ("Critical") and a
//! look unlike every other fx dialog. This renderer draws them with the fx
//! tokens, in the interface language. Enter picks the first button, Esc the
//! last (the cancel one, by fx's convention).

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, PromptButton, PromptHandle,
    PromptLevel, PromptResponse, RenderablePromptHandle, Window, div, prelude::*, px,
};

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
    let view = cx.new(|cx| FxPrompt {
        level,
        message: message.to_owned(),
        detail: detail.map(str::to_owned),
        actions: actions.iter().map(|a| a.label().to_string()).collect(),
        focus: cx.focus_handle(),
        theme,
    });
    handle.with_view(view, window, cx)
}

struct FxPrompt {
    level: PromptLevel,
    message: String,
    detail: Option<String>,
    actions: Vec<String>,
    focus: FocusHandle,
    theme: Theme,
}

impl EventEmitter<PromptResponse> for FxPrompt {}

impl Focusable for FxPrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Which button a key picks: Enter the first, Esc the last.
fn answer_for(key: &str, buttons: usize) -> Option<usize> {
    match key {
        "enter" if buttons > 0 => Some(0),
        "escape" if buttons > 0 => Some(buttons - 1),
        _ => None,
    }
}

impl FxPrompt {
    fn on_key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = answer_for(e.keystroke.key.as_str(), self.actions.len()) {
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
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
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
    use super::answer_for;

    #[test]
    fn enter_confirms_and_escape_cancels() {
        assert_eq!(answer_for("enter", 2), Some(0));
        assert_eq!(answer_for("escape", 2), Some(1));
        assert_eq!(answer_for("x", 2), None);
        assert_eq!(answer_for("enter", 0), None);
    }
}

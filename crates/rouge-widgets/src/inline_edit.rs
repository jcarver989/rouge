use crate::{tokens::RADIUS_INNER, typography::UiStyles};
use rouge_core::Theme;
use rouge_core::{Key, Styled, View, component, div, editable, px, view};
use rouge_reactive::{Callback, Signal};

#[component]
pub fn inline_edit(
    value: Signal<String>,
    #[prop(default, into)] on_commit: Callback<(String,)>,
    #[prop(default, into)] on_cancel: Callback,
) -> impl View {
    let theme = Theme::current();
    view! {
        <div w_full min_w=0.0 px=4.0 rounded=RADIUS_INNER border=px(1.0) border_color=theme.accent bg=theme.canvas
             on_key_down={move |event| {
                 if event.key == Key::Escape {
                     event.stop_propagation();
                     on_cancel.call();
                 }
             }}>
            <editable value test_id="inline-edit" autofocus select_all_on_focus body
                      color=theme.text w_full on_submit={move |value| on_commit.call(value.to_owned())}/>
        </div>
    }
}

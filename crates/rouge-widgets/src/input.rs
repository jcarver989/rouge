use crate::tokens::RADIUS_CONTROL;
use crate::typography::UiStyles;
use rouge_core::{
    Apply, Cursor, Div, Editable, Styled, Theme, component, div, editable, focus_handle, px, view,
};
use rouge_reactive::{Callback, Signal};
use std::ops::Range;

#[component]
#[track_caller]
#[allow(clippy::needless_pass_by_value)]
pub fn text_input(
    value: Signal<String>,
    #[prop(into)] placeholder: Option<String>,
    selection: Option<Signal<Range<usize>>>,
    min_rows: Option<usize>,
    max_rows: Option<usize>,
    #[prop(default)] multiline: bool,
    #[prop(default)] submit_on_enter: bool,
    #[prop(default, into)] on_submit: Callback<(String,)>,
) -> Div {
    let theme = Theme::current();
    let focus = focus_handle();
    let field = editable(value)
        .w_full()
        .body()
        .color(theme.text)
        .caret_color(theme.accent.get_untracked())
        .track_focus(focus)
        .submit_on_enter(submit_on_enter)
        .on_submit(move |text| on_submit.call(text.to_owned()))
        .when(multiline, Editable::multiline)
        .when_some(placeholder, |field, hint| {
            field.placeholder(hint, theme.text_muted)
        })
        .when_some(selection, Editable::selection)
        .when_some(min_rows, Editable::min_rows)
        .when_some(max_rows, Editable::max_rows);
    let fixed = min_rows.is_some() || max_rows.is_some();
    view! {
        <div flex px=8.0 py=6.0 border=px(1.0) rounded=RADIUS_CONTROL bg=theme.elevated overflow_hidden
             cursor=Cursor::Text when:items_start=multiline when:items_center={!multiline}
             when:min_h=(multiline && !fixed, 72.0)
             border_color={move || if focus.is_focused() { theme.accent.get() } else { theme.border.get() }}
             on_pointer_down={move |_| focus.focus()}>
            {div().flex_1().min_w(0.0).child(field)}
        </div>
    }
}

#[component]
#[track_caller]
#[allow(clippy::needless_pass_by_value)]
pub fn text_area(
    value: Signal<String>,
    #[prop(into)] placeholder: Option<String>,
    selection: Option<Signal<Range<usize>>>,
    min_rows: Option<usize>,
    max_rows: Option<usize>,
    #[prop(default)] submit_on_enter: bool,
    #[prop(default, into)] on_submit: Callback<(String,)>,
) -> Div {
    text_input(
        value,
        placeholder,
        selection,
        min_rows,
        max_rows,
        true,
        submit_on_enter,
        on_submit,
    )
}

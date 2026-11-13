use crate::typography::UiStyles;
use crate::util::{focus_ring, pressable};
use rouge_core::{Color, Div, Spring, Styled, Theme, component, div, px, show, text, view};
use rouge_reactive::{Callback, Signal};

#[component]
#[track_caller]
pub fn toggle(checked: Signal<bool>) -> Div {
    let theme = Theme::current();
    let track = view! {
        <div flex_none w=36.0 h=20.0 rounded=10.0 border=px(1.0) border_color=Color::TRANSPARENT
             bg={move || if checked.get() { theme.accent.get() } else { theme.border.get() }}/>
    };
    let flip = Callback::from(move || checked.update(|on| *on = !*on));
    focus_ring(pressable(track, true, true, flip))
        .layer()
        .transition(Spring::snappy())
        .child(view! {
            <div size=16.0 m=1.0 rounded=8.0 bg=Color::WHITE transition=Spring::snappy()
                 translate_x={move || if checked.get() { 16.0 } else { 0.0 }}/>
        })
}

#[component]
#[track_caller]
pub fn radio<T: Clone + PartialEq + 'static>(
    selected: Signal<T>,
    value: T,
    #[prop(into)] label: Option<String>,
) -> Div {
    let theme = Theme::current();
    let chosen = value.clone();
    let on = move || selected.with(|current| *current == value);
    let pick = Callback::from(move || {
        selected.set_if_changed(chosen.clone());
    });
    let root = view! {
        <div flex items_center flex_none gap=8.0 rounded=8.0 border=px(1.0) border_color=Color::TRANSPARENT>
            <div flex items_center justify_center flex_none size=16.0 rounded=8.0 border=px(1.0) bg=theme.elevated
                 border_color={let on = on.clone(); move || if on() { theme.accent.get() } else { theme.border.get() }}>
                <show when=on><div size=8.0 rounded=4.0 bg=theme.accent/></show>
            </div>
            {label.map(|label| view! { <text body primary>{label}</text> })}
        </div>
    };
    focus_ring(pressable(root, true, true, pick))
}

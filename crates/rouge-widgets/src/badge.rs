use crate::tokens::{Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use rouge_core::{Div, Styled, Theme, component, div, text, view};
use rouge_theme::NoticeTone;

#[component]
pub fn badge(#[prop(into)] children: String, tone: NoticeTone) -> Div {
    let theme = Theme::current();
    let ink = move || Theme::notice_style(tone).foreground;
    view! {
        <div flex flex_none items_center gap=Space::X1 h=20.0 px=Space::X2 rounded=Radius::Full text_color=ink
             bg={move || theme.resolve(ink()).with_alpha(TINT_ALPHA)}>
            <div flex_none size=4.0 rounded=Radius::Full bg=ink/>
            <text caption no_wrap weight=500.0>{children}</text>
        </div>
    }
}

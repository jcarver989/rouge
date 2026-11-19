use crate::tokens::{Radius, Space};
use crate::typography::UiStyles;
use rouge_core::{Div, Role, Styled, component, div, text, view};

#[component]
pub fn kbd(#[prop(into)] children: String) -> Div {
    view! {
        <div flex flex_none items_center h=16.0 px=Space::X1 rounded=Radius::Inner bg=Role::SurfaceHover border_b=1.0 border_color=Role::Border>
            <text caption muted no_wrap>{children}</text>
        </div>
    }
}

pub fn key_hints<T: Into<String>, U: Into<String>>(hints: impl IntoIterator<Item = (T, U)>) -> Div {
    let hints = hints.into_iter().map(|(keys, action)| {
        view! {
            <div flex flex_none items_center gap=Space::X1>
                <kbd>{keys.into()}</kbd>
                <text caption muted no_wrap>{action.into()}</text>
            </div>
        }
    });
    view! { <div flex flex_wrap items_center gap_x=Space::X3 gap_y=Space::X1>{..hints}</div> }
}

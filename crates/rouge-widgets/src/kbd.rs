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

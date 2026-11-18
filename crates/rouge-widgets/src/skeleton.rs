use crate::effects::shimmering;
use crate::tokens::Radius;
use rouge_core::{Div, Role, Styled, component, div, view};

#[component]
pub fn skeleton() -> Div {
    shimmering(view! { <div h=16.0 rounded=Radius::Inner bg=Role::SurfaceSelected/> })
}

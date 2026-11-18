use crate::tokens::{GRID_COLUMNS, GRID_MARGIN, TINT_ALPHA};
use rouge_core::view;
use rouge_core::{Div, Show, Styled, Theme, component, div, presentation, show};

#[component]
pub fn column_grid() -> Div {
    view! { <div grid grid_cols=GRID_COLUMNS/> }
}

#[component]
pub fn grid_cell(span: u16) -> Div {
    let presentation = presentation();
    view! {
        <div min_w=0.0 px=GRID_MARGIN
             col_span={move || {
                 if presentation.get().is_compact() {
                     GRID_COLUMNS
                 } else {
                     span.min(GRID_COLUMNS)
                 }
             }}/>
    }
}

#[component]
pub fn grid_overlay(visible: impl Fn() -> bool + 'static) -> Show {
    view! {
        <show when=visible>
            {
                let theme = Theme::current();
                let tint = move || theme.get().accent.with_alpha(TINT_ALPHA);
                let columns = (0..GRID_COLUMNS).map(move |_| {
                    view! {
                        <div min_w=0.0 px=GRID_MARGIN h_full pass_through>
                            <div size_full bg=tint pass_through/>
                        </div>
                    }
                });
                view! {
                    <column_grid test_id="grid-overlay" absolute inset=0.0 pass_through>{..columns}</column_grid>
                }
            }
        </show>
    }
}

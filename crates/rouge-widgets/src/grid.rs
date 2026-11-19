use crate::tokens::{GRID_COLUMNS, GRID_GUTTER, GRID_MARGIN, TINT_ALPHA};
use rouge_core::view;
use rouge_core::{Div, Prop, Show, Styled, Theme, component, div, presentation, show};
use rouge_reactive::memo;

#[component]
pub fn column_grid() -> Div {
    view! { <div grid grid_cols=GRID_COLUMNS/> }
}

#[component]
pub fn grid_cell(
    #[prop(reactive)] span: Prop<u16>,
    #[prop(reactive)] start: Option<Prop<u16>>,
) -> Div {
    let presentation = presentation();
    let compact = move || presentation.get().is_compact();
    let cell = view! {
        <div min_w=0.0 px=GRID_MARGIN
             col_span={move || {
                 if compact() {
                     GRID_COLUMNS
                 } else {
                     span.get().min(GRID_COLUMNS)
                 }
             }}/>
    };
    match start {
        Some(start) => cell.col_start(move || if compact() { 0 } else { start.get() }),
        None => cell,
    }
}

pub fn reading_span(width: f32, max_width: f32) -> u16 {
    if width <= 0.0 {
        return UNMEASURED_READING_SPAN;
    }
    let column = width / f32::from(GRID_COLUMNS);
    let readable = |span: &u16| f32::from(*span) * column - GRID_GUTTER.px() <= max_width;
    (NARROWEST_READING_SPAN..=GRID_COLUMNS)
        .rev()
        .step_by(2)
        .find(readable)
        .unwrap_or(NARROWEST_READING_SPAN)
}

#[component]
pub fn reading_cell(#[prop(reactive)] span: Prop<u16>) -> Div {
    let span = memo(move || span.get());
    view! { <grid_cell span={move || span.get()} start={move || (GRID_COLUMNS - span.get()) / 2 + 1}/> }
}

const NARROWEST_READING_SPAN: u16 = 4;
const UNMEASURED_READING_SPAN: u16 = 8;

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

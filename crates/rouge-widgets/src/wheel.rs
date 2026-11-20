use crate::snap::Snap;
use crate::tokens::Radius;
use crate::typography::UiStyles;
use crate::util::{TestId, chrome};
use rouge_core::{
    BuildCx, DelegateStyle, DragAxis, Fill, Role, StyleDelta, Styled, View, component, div, text,
    view,
};
use rouge_reactive::Signal;

pub struct Wheel {
    selected: Signal<usize>,
    labels: Vec<String>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn wheel<T: Into<String>>(
    selected: Signal<usize>,
    labels: impl IntoIterator<Item = T>,
) -> Wheel {
    Wheel {
        selected,
        labels: labels.into_iter().map(Into::into).collect(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

chrome!(Wheel);

impl View for Wheel {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (selected, id) = (self.selected, self.test_id);
        let snap = Snap {
            axis: DragAxis::Vertical,
            selected,
            last: self.labels.len().saturating_sub(1),
            lead: MIDDLE * ROW,
            reach: f32::INFINITY,
        };
        let rows: Vec<_> = self
            .labels
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                let ink = move || {
                    Fill::Role(if selected.get() == index {
                        Role::Text
                    } else {
                        Role::TextMuted
                    })
                };
                view! {
                    <div test_id?=id.part(format_args!("row-{index}")) flex flex_none items_center justify_center h=ROW
                         on_click={move |_| { selected.set_if_changed(index); }}>
                        <text reading no_wrap color=ink>{label}</text>
                    </div>
                }
            })
            .collect();
        let band = view! {
            <div test_id?=id.part("band") absolute left=0.0 right=0.0 top={MIDDLE * ROW} h=ROW pass_through
                 rounded=Radius::Control bg=Role::SurfaceSelected/>
        };
        let root = view! {
            <div test_id?=id relative flex_none h={ROWS * ROW} min_w=ROW overflow_hidden rounded=Radius::Control/>
        };
        let (root, y) = snap.track(root, || ROW, || true);
        let track = view! { <div flex col translate_y=y>{rows}</div> };
        self.style.apply(root.child(band).child(track)).build(cx);
    }
}

const ROW: f32 = 36.0;
const ROWS: f32 = 5.0;
const MIDDLE: f32 = 2.0;

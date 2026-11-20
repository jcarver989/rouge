use crate::snap::Snap;
use crate::tokens::{Pace, Radius, Space};
use crate::util::{TestId, chrome};
use rouge_core::{
    AnyView, BuildCx, DelegateStyle, Div, DragAxis, Fill, LayoutTransition, Role, Size, StyleDelta,
    Styled, View, component, div, view,
};
use rouge_reactive::{Signal, signal};

pub struct Pager {
    page: Signal<usize>,
    pages: Vec<AnyView>,
    peek: Space,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn pager(page: Signal<usize>) -> Pager {
    Pager {
        page,
        pages: Vec::new(),
        peek: Space::X4,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Pager {
    pub fn peek(mut self, peek: Space) -> Self {
        self.peek = peek;
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.pages.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.pages.extend(children.into_iter().map(View::into_any));
        self
    }
}

chrome!(Pager);

impl View for Pager {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (id, peek, size) = (self.test_id, self.peek.px(), signal(Size::ZERO));
        let snap = Snap {
            axis: DragAxis::Horizontal,
            selected: self.page,
            last: self.pages.len().saturating_sub(1),
            lead: 0.0,
            reach: 1.0,
        };
        let width = move || (size.get().width - 2.0 * peek).max(0.0);
        let pages: Vec<_> = self
            .pages
            .into_iter()
            .enumerate()
            .map(|(index, content)| {
                view! {
                    <div test_id?=id.part(format_args!("page-{index}")) flex col flex_none w=width>{content}</div>
                }
            })
            .collect();
        let root = view! {
            <div test_id?=id relative overflow_hidden rounded=Radius::Control track_size=size/>
        };
        let (root, x) = snap.track(
            root,
            move || width() + GAP.px(),
            move || size.get().width > 0.0,
        );
        let track = view! { <div flex gap=GAP px=self.peek translate_x=x>{pages}</div> };
        self.style.apply(root.child(track)).build(cx);
    }
}

#[component]
pub fn page_dots(page: Signal<usize>, count: usize) -> Div {
    let dots = (0..count).map(|index| {
        let on = move || page.get() == index;
        let dot = view! {
            <div flex_none h=DOT rounded=Radius::Full w={move || if on() { DOT_ON } else { DOT }}
                 bg={move || Fill::Role(if on() { Role::Accent } else { Role::SurfaceSelected })}
                 layout_transition=LayoutTransition::new(Pace::Move).size()/>
        };
        view! {
            <div test_id="page-dot" flex items_center justify_center h=DOT_TARGET on_click={move |_| { page.set_if_changed(index); }}>
                {dot}
            </div>
        }
    });
    view! { <div flex flex_none items_center justify_center gap=Space::X2>{..dots}</div> }
}

const GAP: Space = Space::X3;
const DOT: f32 = 8.0;
const DOT_ON: f32 = 20.0;
const DOT_TARGET: f32 = 24.0;

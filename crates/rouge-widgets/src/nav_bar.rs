use crate::components::{IconButton, icon_button};
use crate::icon::IconName;
use crate::nav_stack::NavBack;
use crate::tokens::{CONTROL_HEIGHT_LARGE, Pace, Space};
use crate::typography::UiStyles;
use crate::util::{TestId, chrome};
use rouge_core::{
    AnyView, Apply, BuildCx, DelegateStyle, Fill, IntoProp, LayoutTransition, Prop, Role,
    ScrollState, StyleDelta, Styled, View, auto, component, div, text, view,
};
use rouge_reactive::{Signal, effect, signal, use_context};
use std::rc::Rc;

pub struct NavBar {
    title: Prop<String>,
    large: bool,
    scroll: Option<Signal<ScrollState>>,
    leading: Option<AnyView>,
    trailing: Vec<AnyView>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn nav_bar<T>(title: impl IntoProp<String, T>) -> NavBar {
    NavBar {
        title: title.into_prop(),
        large: false,
        scroll: None,
        leading: None,
        trailing: Vec::new(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl NavBar {
    pub fn large(mut self) -> Self {
        self.large = true;
        self
    }

    pub fn scroll(mut self, scroll: Signal<ScrollState>) -> Self {
        self.scroll = Some(scroll);
        self
    }

    pub fn leading(mut self, leading: impl View) -> Self {
        self.leading = Some(leading.into_any());
        self
    }

    pub fn trailing(mut self, trailing: impl View) -> Self {
        self.trailing.push(trailing.into_any());
        self
    }
}

chrome!(NavBar);

impl View for NavBar {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (title, id, large) = (Rc::new(self.title), self.test_id, self.large);
        let scrolled = signal(false);
        if let Some(state) = self.scroll {
            effect(move || {
                let offset = state.get().offset.y;
                if offset > FOLD_AFTER {
                    scrolled.set_if_changed(true);
                } else if offset <= 0.0 {
                    scrolled.set_if_changed(false);
                }
            });
        }
        let folded = move || !large || scrolled.get();
        let back = use_context::<NavBack>().map(|back| {
            icon_button(IconName::ChevronLeft, "Back".to_owned())
                .color(Role::Accent)
                .on_click(back.0)
                .when_some(id.part("back"), IconButton::test_id)
                .into_any()
        });
        let inline = {
            let title = title.clone();
            view! {
                <div test_id?=id.part("title") absolute left=TITLE_INSET right=TITLE_INSET top=0.0 bottom=0.0 flex
                     items_center justify_center pass_through
                     opacity={move || if folded() { 1.0 } else { 0.0 }} transition=Pace::Move>
                    <text title primary no_wrap>{move || title.get()}</text>
                </div>
            }
        };
        let headline = large.then(|| {
            view! {
                <div test_id?=id.part("large") flex flex_none items_center px=Space::X4 overflow_hidden
                     h={move || if scrolled.get() { 0.0 } else { LARGE_HEIGHT }}
                     opacity={move || if scrolled.get() { 0.0 } else { 1.0 }} transition=Pace::Move
                     layout_transition=LayoutTransition::new(Pace::Layout).size()>
                    <text display primary no_wrap>{move || title.get()}</text>
                </div>
            }
        });
        let edge = move || {
            Fill::Role(if scrolled.get() {
                Role::BorderSubtle
            } else {
                Role::Canvas
            })
        };
        let root = view! {
            <div test_id?=id flex col flex_none bg=Role::Canvas border_b=1.0 border_color=edge
                 layout_transition=LayoutTransition::new(Pace::Layout).size()>
                <div relative flex flex_none items_center gap=Space::X1 h=CONTROL_HEIGHT_LARGE px=Space::X1>
                    {self.leading.or(back)}
                    {inline}
                    <div flex flex_none items_center gap=Space::X1 ml=auto()>{self.trailing}</div>
                </div>
                {headline}
            </div>
        };
        self.style.apply(root).build(cx);
    }
}

const TITLE_INSET: f32 = 88.0;
const LARGE_HEIGHT: f32 = 40.0;
const FOLD_AFTER: f32 = 24.0;

use crate::icon::{Icon, IconName};
use crate::material::{Material, MaterialExt};
use crate::tokens::{BAR_HEIGHT, CONTROL_HEIGHT_LARGE, Pace, Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::{TestId, focus_ring, follower};
use rouge_core::Role;
use rouge_core::{
    AnyView, Apply, BuildCx, Color, Cursor, DelegateStyle, Div, Fill, Key, Pose, Rect, Styled,
    Text, Theme, View, component, div, each, presentation, safe_area, show, text, view,
};
use rouge_reactive::{Signal, memo, signal};
use std::rc::Rc;

type Content = Rc<dyn Fn() -> AnyView>;

#[derive(Clone)]
struct Label {
    text: String,
    icon: Option<IconName>,
}

pub struct Tabs {
    root: Div,
    selected: Signal<usize>,
    #[expect(clippy::struct_field_names)]
    tabs: Vec<(Label, Content)>,
    bar: bool,
    test_id: Option<String>,
}

#[component]
pub fn tabs(selected: Signal<usize>) -> Tabs {
    Tabs {
        root: view! { <div relative flex col/> },
        selected,
        tabs: Vec::new(),
        bar: false,
        test_id: None,
    }
}

impl Tabs {
    pub fn tab<T: View>(self, label: impl Into<String>, content: impl Fn() -> T + 'static) -> Self {
        self.push(label.into(), None, content)
    }

    pub fn icon_tab<T: View>(
        self,
        icon: IconName,
        label: impl Into<String>,
        content: impl Fn() -> T + 'static,
    ) -> Self {
        self.push(label.into(), Some(icon), content)
    }

    fn push<T: View>(
        mut self,
        text: String,
        icon: Option<IconName>,
        content: impl Fn() -> T + 'static,
    ) -> Self {
        self.tabs
            .push((Label { text, icon }, Rc::new(move || content().into_any())));
        self
    }

    pub fn bar(mut self) -> Self {
        self.bar = true;
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }
}

impl DelegateStyle for Tabs {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.root
    }
}

#[derive(Clone)]
struct Rail {
    labels: Rc<[Label]>,
    selected: Signal<usize>,
    test_id: Option<String>,
}

impl Rail {
    fn header(&self, index: usize, spot: Signal<Rect>) -> Div {
        let (selected, count) = (self.selected, self.labels.len().max(1));
        view! {
            <div test_id?=self.test_id.part(format_args!("tab-{index}")) border=1.0
                 border_color=Color::TRANSPARENT rounded=Radius::Inner focusable track_bounds=spot
                 cursor=Cursor::Pointer on_click={move |_| selected.set(index)}
                 on_key_down={move |event| {
                     let current = selected.get_untracked().min(count - 1);
                     match event.key {
                         Key::ArrowRight => selected.set((current + 1) % count),
                         Key::ArrowLeft => selected.set((current + count - 1) % count),
                         _ => return,
                     }
                     event.stop_propagation();
                 }}/>
        }
    }

    fn spots(&self) -> Rc<[Signal<Rect>]> {
        self.labels.iter().map(|_| signal(Rect::ZERO)).collect()
    }

    fn headers(
        &self,
        spots: &[Signal<Rect>],
        on: Role,
        layout: fn(Div) -> Div,
        scale: fn(Text) -> Text,
    ) -> impl Iterator<Item = Div> {
        let selected = self.selected;
        self.labels.iter().enumerate().map(move |(index, label)| {
            let ink = move || {
                Fill::Role(if selected.get() == index {
                    on
                } else {
                    Role::TextMuted
                })
            };
            let caption = scale(view! { <text no_wrap color=ink>{label.text.clone()}</text> });
            let header = layout(self.header(index, spots[index]))
                .child(label.icon.map(|icon| Icon::new(icon, ink)))
                .child(caption);
            focus_ring(header)
        })
    }

    fn strip(self) -> Div {
        let (strip, spots, selected) = (signal(Rect::ZERO), self.spots(), self.selected);
        let underline = follower(
            strip,
            {
                let spots = spots.clone();
                move || {
                    let at = spots.get(selected.get())?.get();
                    Some(Rect::new(
                        at.origin.x,
                        at.max().y - UNDERLINE,
                        at.size.width,
                        UNDERLINE,
                    ))
                }
            },
            {
                let id = self.test_id.part("indicator");
                move |layer| {
                    layer
                        .bg(Role::Accent)
                        .rounded(Radius::Full)
                        .when_some(id, Div::test_id)
                }
            },
        );
        let headers = self.headers(
            &spots,
            Role::Text,
            |header| header.flex().items_center().gap(Space::X2).px(Space::X3),
            UiStyles::body,
        );
        view! {
            <div relative flex flex_none h=BAR_HEIGHT border_b=1.0 border_color=Role::BorderSubtle track_bounds=strip>
                {..headers}
                {underline}
            </div>
        }
    }

    fn bar(self) -> Div {
        let (theme, selected, safe) = (Theme::current(), self.selected, safe_area());
        let (bar, spots) = (signal(Rect::ZERO), self.spots());
        let pill = follower(
            bar,
            {
                let spots = spots.clone();
                move || Some(spots.get(selected.get())?.get())
            },
            {
                let id = self.test_id.part("indicator");
                move |layer| {
                    layer
                        .pass_through()
                        .bg(move || theme.get().accent.with_alpha(TINT_ALPHA))
                        .rounded(Radius::Control)
                        .when_some(id, Div::test_id)
                }
            },
        );
        let headers = self.headers(
            &spots,
            Role::Accent,
            |header| {
                header
                    .flex()
                    .col()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .h(CONTROL_HEIGHT_LARGE)
                    .rounded(Radius::Control)
            },
            UiStyles::caption,
        );
        view! {
            <div test_id?=self.test_id.part("bar") absolute left=BAR_INSET right=BAR_INSET
                 bottom={move || BAR_INSET + safe.get().bottom} flex items_center gap=Space::X1 p=Space::X1
                 material=Material::Glass track_bounds=bar>
                {..headers}
                {pill}
            </div>
        }
    }
}

impl View for Tabs {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (selected, floats) = (self.selected, self.bar);
        let (labels, contents): (Vec<_>, Vec<_>) = self.tabs.into_iter().unzip();
        let contents: Rc<[Content]> = contents.into();
        let rail = Rail {
            labels: labels.into(),
            selected,
            test_id: self.test_id,
        };
        let presentation = presentation();
        let handheld = memo(move || floats && presentation.get().is_handheld());
        let (top, bottom) = (rail.clone(), rail);
        let strip = view! { <show when={move || !handheld.get()}>{top.clone().strip()}</show> };
        let bar = view! { <show when={move || handheld.get()}>{bottom.clone().bar()}</show> };
        let hidden = Pose::new().opacity(0.0);
        let body = view! {
            <each items={move || vec![selected.get()]} key={|index| *index} let:index
                 enter=(hidden, Pace::Enter) exit=(hidden, Pace::Exit)>
                {{ match contents.get(index.get_untracked()) {
                    Some(content) => content(),
                    None => view! { <div/> }.into_any(),
                } }}
            </each>
        };
        self.root
            .child(strip)
            .child(view! { <div flex_1 min_h=0.0>{body}</div> })
            .child(bar)
            .build(cx);
    }
}

const UNDERLINE: f32 = 2.0;
const BAR_INSET: f32 = 12.0;

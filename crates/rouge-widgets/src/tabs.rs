use crate::dock::{BAR_INSET, Dock, Tier};
use crate::icon::{Icon, IconName};
use crate::material::{Material, MaterialExt};
use crate::tokens::{BAR_HEIGHT, CONTROL_HEIGHT_LARGE, Pace, Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::{TestId, focus_ring, follower};
use rouge_core::Role;
use rouge_core::access;
use rouge_core::{
    AnyView, Apply, BuildCx, Color, Cursor, DelegateStyle, Div, Fill, IntoProp, Key, Pose, Prop,
    Rect, Styled, Text, Theme, View, auto, component, div, each, keyboard_inset, pct, presentation,
    safe_area, show, text, view,
};
use rouge_reactive::{Signal, memo, signal};
use std::rc::Rc;

type Content = Rc<dyn Fn() -> AnyView>;

#[derive(Clone)]
struct Label {
    text: String,
    icon: Option<IconName>,
    badge: Option<Rc<Prop<usize>>>,
    dot: Option<Rc<Prop<bool>>>,
}

pub struct Tabs {
    root: Div,
    selected: Signal<usize>,
    #[expect(clippy::struct_field_names)]
    tabs: Vec<(Label, Content)>,
    accessory: Option<Content>,
    bar: bool,
    test_id: Option<String>,
}

#[component]
pub fn tabs(selected: Signal<usize>) -> Tabs {
    Tabs {
        root: view! { <div relative flex col/> },
        selected,
        tabs: Vec::new(),
        accessory: None,
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
        let label = Label {
            text,
            icon,
            badge: None,
            dot: None,
        };
        self.tabs
            .push((label, Rc::new(move || content().into_any())));
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

    pub fn badge<T>(mut self, count: impl IntoProp<usize, T>) -> Self {
        if let Some((label, _)) = self.tabs.last_mut() {
            label.badge = Some(Rc::new(count.into_prop()));
        }
        self
    }

    pub fn dot<T>(mut self, on: impl IntoProp<bool, T>) -> Self {
        if let Some((label, _)) = self.tabs.last_mut() {
            label.dot = Some(Rc::new(on.into_prop()));
        }
        self
    }

    pub fn accessory<T: View>(mut self, accessory: impl Fn() -> T + 'static) -> Self {
        self.accessory = Some(Rc::new(move || accessory().into_any()));
        self
    }
}

impl DelegateStyle for Tabs {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.root
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Strip,
    Bar,
    Rail,
}

#[derive(Clone)]
struct Rail {
    labels: Rc<[Label]>,
    selected: Signal<usize>,
    accessory: Option<Content>,
    test_id: Option<String>,
}

impl Rail {
    fn header(&self, index: usize, spot: Signal<Rect>) -> Div {
        let (selected, count) = (self.selected, self.labels.len().max(1));
        view! {
            <div test_id?=self.test_id.part(format_args!("tab-{index}")) role=access::Role::Tab
                 access_label={self.labels[index].text.clone()} is_selected={move || selected.get() == index}
                 relative border=1.0
                 border_color=Color::TRANSPARENT rounded=Radius::Inner focusable track_bounds=spot
                 cursor=Cursor::Pointer on_click={move |_| selected.set(index)}
                 on_key_down={move |event| {
                     let current = selected.get_untracked().min(count - 1);
                     match event.key {
                         Key::ArrowRight | Key::ArrowDown => selected.set((current + 1) % count),
                         Key::ArrowLeft | Key::ArrowUp => selected.set((current + count - 1) % count),
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
        floating: bool,
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
                .child(caption)
                .child(self.marker(index, label, floating));
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
            || false,
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
            false,
            |header| header.flex().items_center().gap(Space::X2).px(Space::X3),
            UiStyles::body,
        );
        let accessory = self.slot(view! { <div ml=auto() px=Space::X3/> });
        view! {
            <div relative flex flex_none h=BAR_HEIGHT border_b=1.0 border_color=Role::BorderSubtle track_bounds=strip>
                {..headers}
                {accessory}
                {underline}
            </div>
        }
    }

    fn bar(self, dock: Dock) -> Div {
        let safe = safe_area();
        let (bar, spots) = (signal(Rect::ZERO), self.spots());
        dock.hold(Tier::Tabs, move || {
            let height = bar.get().size.height;
            if height > 0.0 {
                height + BAR_INSET + safe.get().bottom
            } else {
                0.0
            }
        });
        let (headers, pill) = self.stacked(bar, &spots, |header| {
            header.flex_1().h(CONTROL_HEIGHT_LARGE)
        });
        let accessory = self.slot(view! {
            <div flex_none min_h=CONTROL_HEIGHT_LARGE px=Space::X2 border_b=1.0 border_color=Role::BorderSubtle/>
        });
        view! {
            <div test_id?=self.test_id.part("bar") absolute left=BAR_INSET right=BAR_INSET
                 bottom={move || BAR_INSET + safe.get().bottom} flex col gap=Space::X1 p=Space::X1
                 material=Material::Glass track_bounds=bar>
                {accessory}
                <div flex items_center gap=Space::X1>{headers}</div>
                {pill}
            </div>
        }
    }

    fn marker(&self, index: usize, label: &Label, floating: bool) -> Option<Div> {
        if label.badge.is_none() && label.dot.is_none() {
            return None;
        }
        let (badge, dot) = (label.badge.clone(), label.dot.clone());
        let count = move || badge.as_ref().map_or(0, |count| count.get());
        let id = self.test_id.part(format_args!("tab-{index}-badge"));
        let (counted, labelled, pill_id) = (count.clone(), count.clone(), id.clone());
        let pill = show(
            move || counted() > 0,
            move || {
                let count = labelled.clone();
                let shown = move || match count() {
                    count if count > BADGE_LIMIT => format!("{BADGE_LIMIT}+"),
                    count => count.to_string(),
                };
                view! {
                    <div test_id?={pill_id.clone()} flex flex_none items_center justify_center min_w=16.0 h=16.0
                         px=Space::X1 rounded=Radius::Full bg=Role::Accent>
                        <text caption no_wrap color=Role::AccentForeground>{shown}</text>
                    </div>
                }
            },
        );
        let bare = show(
            move || count() == 0 && dot.as_ref().is_some_and(|dot| dot.get()),
            move || view! { <div test_id?={id.clone()} flex_none size=8.0 rounded=Radius::Full bg=Role::Accent/> },
        );
        let marker = view! { <div flex flex_none pass_through>{pill}{bare}</div> };
        Some(if floating {
            marker
                .absolute()
                .top(Space::X1)
                .left(pct(50.0))
                .ml(Space::X1)
        } else {
            marker
        })
    }

    fn pill(&self, container: Signal<Rect>, spots: &Rc<[Signal<Rect>]>) -> impl View + use<> {
        let (theme, selected, spots) = (Theme::current(), self.selected, spots.clone());
        let id = self.test_id.part("indicator");
        follower(
            container,
            move || Some(spots.get(selected.get())?.get()),
            || false,
            move |layer| {
                layer
                    .pass_through()
                    .bg(move || theme.get().accent.with_alpha(TINT_ALPHA))
                    .rounded(Radius::Control)
                    .when_some(id, Div::test_id)
            },
        )
    }

    fn rail(self) -> Div {
        let (rail, spots) = (signal(Rect::ZERO), self.spots());
        let (headers, pill) = self.stacked(rail, &spots, |header| header.flex_none().h(RAIL_ITEM));
        view! {
            <div test_id?=self.test_id.part("rail") relative flex col flex_none gap=Space::X1 p=Space::X1
                 w=RAIL_WIDTH border_r=1.0
                 border_color=Role::BorderSubtle bg=Role::Chrome track_bounds=rail>
                {headers}
                {pill}
            </div>
        }
    }

    fn footer(self) -> Option<Div> {
        self.slot(view! {
            <div flex_none min_h=CONTROL_HEIGHT_LARGE px=Space::X3 border_t=1.0 border_color=Role::BorderSubtle
                 bg=Role::Chrome/>
        })
    }

    fn slot(&self, frame: Div) -> Option<Div> {
        let accessory = self.accessory.as_ref()?;
        let slot = frame.flex().items_center().child(accessory());
        Some(slot.when_some(self.test_id.part("accessory"), Div::test_id))
    }

    fn stacked(
        &self,
        container: Signal<Rect>,
        spots: &Rc<[Signal<Rect>]>,
        size: fn(Div) -> Div,
    ) -> (Vec<Div>, impl View + use<>) {
        let headers = self.headers(
            spots,
            Role::Accent,
            true,
            |header| {
                header
                    .flex()
                    .col()
                    .items_center()
                    .justify_center()
                    .rounded(Radius::Control)
            },
            UiStyles::caption,
        );
        (headers.map(size).collect(), self.pill(container, spots))
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
            accessory: self.accessory,
            test_id: self.test_id,
        };
        let (root, dock) = (self.root, Dock::current());
        let (presentation, keyboard) = (presentation(), keyboard_inset());
        let form = memo(move || {
            let presentation = presentation.get();
            match (floats && presentation.is_touch(), presentation.is_compact()) {
                (false, _) => Form::Strip,
                (true, true) => Form::Bar,
                (true, false) => Form::Rail,
            }
        });
        let docked =
            memo(move || form.get() == Form::Bar && keyboard.get() == 0.0 && !dock.covered());
        let (top, bottom, side, under) = (rail.clone(), rail.clone(), rail.clone(), rail);
        let strip = show(
            move || form.get() == Form::Strip,
            move || top.clone().strip(),
        );
        let bar = show(move || docked.get(), move || bottom.clone().bar(dock));
        let side = show(
            move || form.get() == Form::Rail,
            move || side.clone().rail(),
        );
        let footer = show(
            move || form.get() == Form::Rail,
            move || under.clone().footer(),
        );
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
        let middle = view! {
            <div flex flex_1 min_h=0.0>
                {side}
                <div flex col flex_1 min_w=0.0 min_h=0.0>
                    <div flex_1 min_h=0.0>{body}</div>
                    {footer}
                </div>
            </div>
        };
        root.child(strip).child(middle).child(bar).build(cx);
    }
}

const UNDERLINE: f32 = 2.0;
const BADGE_LIMIT: usize = 99;
const RAIL_WIDTH: f32 = 80.0;
const RAIL_ITEM: f32 = 56.0;

use crate::{
    effects::{self, RIPPLE_SECONDS, Ripple},
    icon::{Icon, IconName},
    material::{Material, MaterialExt},
    popover::{Hint, POPPED, Placement, Tooltip, dismiss_layer},
    progress::spinning_ring,
    sheet::{Detent, Presenter},
    tokens::{self, Hold, MotionScale, Pace, Space},
    tree::indent,
    typography::UiStyles,
    util::{
        Spot, TestId, by_touch, chrome, fade_when_disabled, focus_ring, hit_box, row_height,
        tone_icon,
    },
};
use rouge_core::Role;
use rouge_core::{
    AnyView, Apply, BackdropBlur, BuildCx, Chord, Color, Cursor, DelegateStyle, Div, DragAxis,
    DragPhase, Fill, FocusPolicy, IntoChord, IntoProp, Key, Length, Path, Point, PointerButton,
    PointerEvent, Pose, Presentation, Prop, Rect, Size, StyleDelta, Styled, Text, Theme, Timing,
    View, auto, canvas, component, div, each, motion, overlay, pct, presentation, px, show, text,
    view,
};
use rouge_reactive::{Callback, Signal, signal, spawn, use_context};
use rouge_theme::{
    ButtonVariant, ControlState, InteractionState, ModalSize, NoticeTone, SelectionState,
};
use std::cell::Cell;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl ControlSize {
    pub fn of(presentation: Presentation) -> Self {
        if presentation.is_touch() {
            ControlSize::Large
        } else {
            ControlSize::Medium
        }
    }

    pub(crate) const fn metrics(self) -> (f32, f32) {
        match self {
            ControlSize::Small => (24.0, tokens::CONTROL_PADDING_X_SMALL),
            ControlSize::Medium => (tokens::ROW_HEIGHT, tokens::CONTROL_PADDING_X_MEDIUM),
            ControlSize::Large => (
                tokens::CONTROL_HEIGHT_LARGE,
                tokens::CONTROL_PADDING_X_LARGE,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Unchecked,
    Checked,
    Indeterminate,
}

use std::rc::Rc;

pub struct Button {
    label: Prop<String>,
    icon: Option<IconName>,
    variant: ButtonVariant,
    size: Option<ControlSize>,
    tooltip: Option<String>,
    disabled: Prop<bool>,
    loading: Prop<bool>,
    confirm: Option<Hold>,
    ripple: bool,
    selected: Prop<bool>,
    focus: FocusPolicy,
    on_click: Callback,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn button<T>(label: impl IntoProp<String, T>) -> Button {
    Button {
        label: label.into_prop(),
        icon: None,
        variant: ButtonVariant::default(),
        size: None,
        tooltip: None,
        disabled: Prop::Static(false),
        loading: Prop::Static(false),
        confirm: None,
        ripple: false,
        selected: Prop::Static(false),
        focus: FocusPolicy::Tab,
        on_click: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Button {
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = Some(size);
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn disabled<T>(mut self, disabled: impl IntoProp<bool, T>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }

    pub fn loading<T>(mut self, loading: impl IntoProp<bool, T>) -> Self {
        self.loading = loading.into_prop();
        self
    }

    pub fn confirm(mut self, hold: Hold) -> Self {
        self.confirm = Some(hold);
        self
    }

    pub fn ripple(mut self) -> Self {
        self.ripple = true;
        self
    }

    pub fn selected<T>(mut self, selected: impl IntoProp<bool, T>) -> Self {
        self.selected = selected.into_prop();
        self
    }

    pub fn focus_policy(mut self, focus: FocusPolicy) -> Self {
        self.focus = focus;
        self
    }

    pub fn on_click(mut self, on_click: impl Into<Callback>) -> Self {
        self.on_click = on_click.into();
        self
    }
}

chrome!(Button);

impl View for Button {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (variant, theme) = (self.variant, Theme::current());
        let look = move |interaction, selected| {
            Theme::control_style(variant, ControlState::new(interaction).selected(selected))
        };
        let fill = move |interaction, selected| {
            move || {
                let rest = look(InteractionState::Rest, false).background;
                let state = look(interaction, selected).background;
                let resolve = |fill: Option<Fill>| theme.resolve(fill.unwrap_or_default());
                resolve(state).over(resolve(rest))
            }
        };
        let ink = move |interaction, selected| move || look(interaction, selected).foreground;
        let edge = move || match variant {
            ButtonVariant::Secondary => theme.resolve(Fill::Role(Role::Border)),
            ButtonVariant::Destructive => theme.get().destructive.with_alpha(OUTLINE_ALPHA),
            ButtonVariant::Primary | ButtonVariant::Ghost => Color::TRANSPARENT,
        };
        let (size, presentation) = (self.size, presentation());
        let metrics = move || {
            size.unwrap_or_else(|| ControlSize::of(presentation.get()))
                .metrics()
        };
        let (height, padding) = (move || metrics().0, move || metrics().1);
        let disabled = Rc::new(self.disabled);
        let loading = Rc::new(self.loading);
        let selected = Rc::new(self.selected);
        let (hovered, pressed) = (signal(false), signal(false));
        let busy = {
            let loading = loading.clone();
            move || loading.get()
        };
        let spinner_id = self.test_id.part("spinner");
        let ring_arc = move || match variant {
            ButtonVariant::Primary => Fill::Role(Role::AccentForeground),
            ButtonVariant::Destructive => Fill::Role(Role::Destructive),
            ButtonVariant::Secondary | ButtonVariant::Ghost => Fill::Role(Role::Accent),
        };
        let spinner = view! {
            <show when=busy.clone() enter=(POPPED, Pace::Enter) exit=(POPPED, Pace::Exit)>
                <div test_id?={spinner_id.clone()} absolute inset=0.0 flex items_center justify_center pass_through>
                    {spinning_ring(tokens::ICON_SIZE, ink(InteractionState::Rest, false), ring_arc)}
                </div>
            </show>
        };
        let icon = self.icon.map(|icon| {
            let selected = selected.clone();
            Icon::new(icon, move || {
                let interaction = if hovered.get() {
                    InteractionState::Hovered
                } else {
                    InteractionState::Rest
                };
                look(interaction, selected.get()).foreground
            })
        });
        let root = view! {
            <div relative flex items_center justify_center flex_none rounded=tokens::Radius::Control
                 border=1.0 border_color=edge h=height px=padding
                 font_weight=500.0 bg={fill(InteractionState::Rest, false)}
                 text_color={ink(InteractionState::Rest, false)}
                 on_hover={move |inside| hovered.set(inside)}
                 on_press={move |down| pressed.set(down)}
                 scale={move || if pressed.get() { tokens::PRESS_SCALE } else { 1.0 }}>
                <div flex items_center gap=Space::X2 opacity={move || if busy() { 0.0 } else { 1.0 }}
                     transition=Pace::Move>
                    {icon}
                    <text label no_wrap>{self.label}</text>
                </div>
                {spinner}
            </div>
        }
        .hover(|style| {
            style
                .bg(fill(InteractionState::Hovered, false))
                .text_color(ink(InteractionState::Hovered, false))
        })
        .selected(|style| {
            style
                .bg(fill(InteractionState::Rest, true))
                .text_color(ink(InteractionState::Rest, true))
                .font_weight(600.0)
        })
        .is_selected({
            let selected = selected.clone();
            move || selected.get()
        });
        let root = fade_when_disabled(focus_ring(root), &disabled)
            .enabled(move || !disabled.get() && !loading.get())
            .transition(Pace::Press);
        let root = match self.confirm {
            Some(hold) => confirming(root, hold, self.on_click, self.focus),
            None => activate(root, self.on_click, self.focus),
        };
        let root = if self.ripple {
            let ripple_id = self.test_id.part("ripple");
            rippling(root, ripple_id, move || {
                let ink = look(InteractionState::Rest, false).foreground;
                theme.resolve(ink).with_alpha(RIPPLE_ALPHA)
            })
        } else {
            root
        };
        self.style
            .apply(root.when_some(self.test_id, Div::test_id))
            .hint(self.tooltip)
            .build(cx);
    }
}

const OUTLINE_ALPHA: f32 = 0.45;

fn confirming(root: Div, hold: Hold, action: Callback, focus: FocusPolicy) -> Div {
    let theme = Theme::current();
    let (bounds, progress) = (signal(Rect::ZERO), motion(-OFFSTAGE));
    let width = move || bounds.get_untracked().size.width;
    let begin = move || {
        progress.set(-width());
        let arrival = progress.animate_to(0.0, hold);
        spawn(async move {
            if arrival.await {
                action.call();
                progress.animate_to(-width(), Pace::Exit);
            }
        });
    };
    let release = move || {
        progress.animate_to(-width(), Pace::Exit);
    };
    let keyed = |key: &Key| matches!(key, Key::Enter | Key::Space);
    let fill = view! {
        <div absolute inset=0.0 pass_through
             bg={move || theme.get().destructive.with_alpha(tokens::TINT_ALPHA)} translate_x=progress/>
    };
    root.overflow_hidden()
        .track_bounds(bounds)
        .cursor(Cursor::Pointer)
        .focus_policy(focus)
        .on_press(move |down| if down { begin() } else { release() })
        .on_key_down(move |event| {
            if keyed(&event.key) {
                event.stop_propagation();
                if !event.repeat {
                    begin();
                }
            }
        })
        .on_key_up(move |event| {
            if keyed(&event.key) {
                release();
            }
        })
        .child(fill)
}

const OFFSTAGE: f32 = 4096.0;

fn rippling(root: Div, test_id: Option<String>, ink: impl Fn() -> Color + 'static) -> Div {
    if MotionScale::current() == MotionScale::None {
        return root;
    }
    let (spot, clock) = (signal(None::<(u64, Point)>), motion(0.0));
    let wave = view! {
        <each items={move || spot.get().into_iter().collect::<Vec<_>>()} key={|(press, _)| *press}
             let:spot>
            {
                let (_, center) = spot.get_untracked();
                effects::overlay(tokens::Radius::Control)
                    .when_some(test_id.clone(), Div::test_id)
                    .effect(Ripple { center, ink: ink() })
            }
        </each>
    };
    root.on_pointer_down(move |event| {
        if event.button != PointerButton::Primary {
            return;
        }
        let press = spot.with_untracked(|spot| spot.map_or(0, |(press, _)| press + 1));
        spot.set(Some((press, event.local())));
        clock.set(0.0);
        let spent = clock.animate_to(1.0, Hold::seconds(RIPPLE_SECONDS));
        spawn(async move {
            if spent.await {
                spot.set(None);
            }
        });
    })
    .child(wave)
}

const RIPPLE_ALPHA: f32 = 0.24;

pub struct IconButton {
    icon: IconName,
    label: String,
    size: Option<ControlSize>,
    icon_size: Option<f32>,
    color: Option<Prop<Fill>>,
    shortcut: Option<Chord>,
    disabled: Prop<bool>,
    selected: Prop<bool>,
    confirm: Option<Hold>,
    focus: FocusPolicy,
    on_click: Callback,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn icon_button(icon: IconName, #[prop(into)] label: String) -> IconButton {
    IconButton {
        icon,
        label,
        size: None,
        icon_size: None,
        color: None,
        shortcut: None,
        disabled: Prop::Static(false),
        selected: Prop::Static(false),
        confirm: None,
        focus: FocusPolicy::Tab,
        on_click: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl IconButton {
    pub fn confirm(mut self, hold: Hold) -> Self {
        self.confirm = Some(hold);
        self
    }

    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = Some(size);
        self
    }

    pub fn color<T>(mut self, color: impl IntoProp<Fill, T>) -> Self {
        self.color = Some(color.into_prop());
        self
    }

    #[track_caller]
    pub fn shortcut(mut self, chord: impl IntoChord) -> Self {
        self.shortcut = Some(chord.into_chord());
        self
    }

    pub fn disabled<T>(mut self, disabled: impl IntoProp<bool, T>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }

    pub fn selected<T>(mut self, selected: impl IntoProp<bool, T>) -> Self {
        self.selected = selected.into_prop();
        self
    }

    pub fn focus_policy(mut self, focus: FocusPolicy) -> Self {
        self.focus = focus;
        self
    }

    pub fn on_click(mut self, on_click: impl Into<Callback>) -> Self {
        self.on_click = on_click.into();
        self
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = Some(size);
        self
    }
}

chrome!(IconButton);

impl View for IconButton {
    fn build(self, cx: &mut BuildCx<'_>) {
        let disabled = Rc::new(self.disabled);
        let selected = Rc::new(self.selected);
        let (hovered, pressed) = (signal(false), signal(false));
        let foreground = {
            let (disabled, selected, color) = (disabled.clone(), selected.clone(), self.color);
            move || match &color {
                Some(color) => color.get(),
                None if selected.get() || (!disabled.get() && hovered.get()) => {
                    Fill::Role(Role::Text)
                }
                None => Fill::Role(Role::TextMuted),
            }
        };
        let control = self
            .size
            .unwrap_or_else(|| ControlSize::of(presentation().get_untracked()));
        let (side, glyph) = match control {
            ControlSize::Large => (tokens::CONTROL_HEIGHT_LARGE, tokens::ICON_SIZE_LARGE),
            ControlSize::Small | ControlSize::Medium => {
                (tokens::ICON_BUTTON_SIZE, tokens::ICON_SIZE)
            }
        };
        let root = view! {
            <div flex flex_none items_center justify_center size=side
                 rounded=tokens::Radius::Control border=1.0 border_color=Color::TRANSPARENT
                 on_hover={move |inside| hovered.set(inside)}
                 on_press={move |down| pressed.set(down)}
                 scale={move || if pressed.get() { tokens::PRESS_SCALE } else { 1.0 }}>
                {Icon::new(self.icon, foreground).size(self.icon_size.unwrap_or(glyph))}
            </div>
        }
        .hover(|style| style.bg(Role::SurfaceHover))
        .selected(|style| style.bg(Role::SurfaceSelected))
        .disabled(|style| style.bg(Color::TRANSPARENT))
        .is_selected(move || selected.get());
        let root = fade_when_disabled(focus_ring(root), &disabled)
            .enabled(move || !disabled.get())
            .transition(Pace::Press);
        let root = match self.confirm {
            Some(hold) => confirming(root, hold, self.on_click, self.focus),
            None => activate(root, self.on_click, self.focus),
        };
        self.style
            .apply(root.when_some(self.test_id, Div::test_id))
            .tooltip(self.label)
            .when_some(self.shortcut, Tooltip::shortcut)
            .build(cx);
    }
}

pub trait CheckModel: Copy + 'static {
    fn state(self) -> CheckState;
    fn toggle(self);
}

impl CheckModel for Signal<bool> {
    fn state(self) -> CheckState {
        if self.get() {
            CheckState::Checked
        } else {
            CheckState::Unchecked
        }
    }

    fn toggle(self) {
        Signal::toggle(&self);
    }
}

impl CheckModel for Signal<CheckState> {
    fn state(self) -> CheckState {
        self.get()
    }

    fn toggle(self) {
        self.update(|state| {
            *state = match state {
                CheckState::Checked => CheckState::Unchecked,
                CheckState::Unchecked | CheckState::Indeterminate => CheckState::Checked,
            }
        });
    }
}

pub struct Checkbox<T> {
    model: T,
    mark_color: Option<Color>,
    tooltip: Option<String>,
    disabled: Prop<bool>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn checkbox<T: CheckModel>(checked: T) -> Checkbox<T> {
    Checkbox {
        model: checked,
        mark_color: None,
        tooltip: None,
        disabled: Prop::Static(false),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl<T> Checkbox<T> {
    pub fn mark_color(mut self, color: Color) -> Self {
        self.mark_color = Some(color);
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn disabled<U>(mut self, disabled: impl IntoProp<bool, U>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }
}

chrome!(impl[T] Checkbox<T>);

impl<T: CheckModel> View for Checkbox<T> {
    fn build(self, cx: &mut BuildCx<'_>) {
        let theme = Theme::current();
        let (model, mark_color) = (self.model, self.mark_color);
        let mark = canvas(move |painter| {
            let size = painter.size();
            let at = |x: f32, y: f32| (size.width * x, size.height * y);
            let path = match model.state() {
                CheckState::Unchecked => return,
                CheckState::Checked => {
                    let ((ax, ay), (bx, by), (cx, cy)) =
                        (at(0.22, 0.5), at(0.43, 0.72), at(0.78, 0.28));
                    Path::builder()
                        .move_to(ax, ay)
                        .line_to(bx, by)
                        .line_to(cx, cy)
                        .build()
                }
                CheckState::Indeterminate => {
                    let ((ax, ay), (bx, by)) = (at(0.25, 0.5), at(0.75, 0.5));
                    Path::builder().move_to(ax, ay).line_to(bx, by).build()
                }
            };
            let color = mark_color.unwrap_or_else(|| theme.get().accent_foreground);
            painter.stroke_round(&path, (size.width / 6.0).min(2.0), color);
        })
        .size_full();
        let drawn = move |shown: f32, hidden: f32| {
            move || {
                if model.state() == CheckState::Unchecked {
                    hidden
                } else {
                    shown
                }
            }
        };
        let mark = view! {
            <div size_full scale=drawn(1.0, MARK_SCALE) opacity=drawn(1.0, 0.0) transition=Pace::Press>
                {mark}
            </div>
        };
        let disabled = Rc::new(self.disabled);
        let marked = move |on: Role, off: Role| {
            move || {
                Fill::Role(if model.state() == CheckState::Unchecked {
                    off
                } else {
                    on
                })
            }
        };
        let side = by_touch(tokens::CHECKBOX_SIZE_LARGE, tokens::CHECKBOX_SIZE);
        let root = view! {
            <div flex_none size=side rounded=tokens::Radius::Inner border=1.0
                 bg={marked(Role::Accent, Role::Elevated)} border_color={marked(Role::Accent, Role::Border)}>
                {mark}
            </div>
        }
        .hover(|style| style.border_color(Role::Accent));
        let root = fade_when_disabled(focus_ring(hit_box(root)), &disabled)
            .enabled(move || !disabled.get())
            .is_checked(move || model.state() == CheckState::Checked)
            .on_activate(move || model.toggle());
        let tooltip = self.tooltip;
        self.style
            .apply(root.when_some(self.test_id, Div::test_id))
            .hint(tooltip)
            .build(cx);
    }
}

const MARK_SCALE: f32 = 0.5;

#[derive(Clone, Copy)]
struct SegmentIndicator(Spot);

pub struct SegmentedControl {
    root: Div,
    indicator: StyleDelta,
    segments: Vec<AnyView>,
    test_id: Option<String>,
}

#[component]
pub fn segmented_control() -> SegmentedControl {
    SegmentedControl {
        root: view! {
            <div relative flex items_center gap=Space::X1 p=Space::X1 rounded=tokens::Radius::Control bg=Role::Chrome/>
        },
        indicator: StyleDelta::default()
            .rounded(tokens::Radius::Inner)
            .bg(Role::Elevated)
            .border(1.0)
            .border_color(Role::BorderSubtle),
        segments: Vec::new(),
        test_id: None,
    }
}

impl SegmentedControl {
    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }

    pub fn indicator(mut self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.indicator = f(self.indicator);
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.segments.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.segments
            .extend(children.into_iter().map(View::into_any));
        self
    }
}

impl DelegateStyle for SegmentedControl {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.root
    }
}

impl View for SegmentedControl {
    fn build(self, cx: &mut BuildCx<'_>) {
        let spot = Spot::new();
        let (style, id) = (self.indicator, self.test_id.part("indicator"));
        let indicator = spot.follower(move |layer| style.apply(layer).when_some(id, Div::test_id));
        spot.contain(self.root)
            .when_some(self.test_id, Div::test_id)
            .provide(SegmentIndicator(spot))
            .child(indicator)
            .children(self.segments)
            .build(cx);
    }
}

pub struct Segment {
    label: String,
    icon: Option<IconName>,
    detail: Option<String>,
    labelled: bool,
    badge: Option<AnyView>,
    selected: Prop<bool>,
    disabled: Prop<bool>,
    on_click: Callback,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn segment(#[prop(into)] label: String) -> Segment {
    Segment {
        label,
        icon: None,
        detail: None,
        labelled: false,
        badge: None,
        selected: Prop::Static(false),
        disabled: Prop::Static(false),
        on_click: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Segment {
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn labelled(mut self) -> Self {
        self.labelled = true;
        self
    }

    pub fn badge(mut self, badge: impl View) -> Self {
        self.badge = Some(badge.into_any());
        self
    }

    pub fn selected<T>(mut self, selected: impl IntoProp<bool, T>) -> Self {
        self.selected = selected.into_prop();
        self
    }

    pub fn disabled<T>(mut self, disabled: impl IntoProp<bool, T>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }

    pub fn on_click(mut self, on_click: impl Into<Callback>) -> Self {
        self.on_click = on_click.into();
        self
    }
}

chrome!(Segment);

impl View for Segment {
    fn build(self, cx: &mut BuildCx<'_>) {
        let Segment {
            label,
            icon,
            detail,
            labelled,
            badge,
            selected,
            disabled,
            on_click,
            test_id,
            style,
        } = self;
        let padding = if icon.is_some() && !labelled {
            tokens::CONTROL_PADDING_X_SMALL
        } else {
            tokens::CONTROL_PADDING_X_MEDIUM
        };
        let detail = detail.filter(|_| icon.is_some()).map(|detail| {
            let detail_id = test_id.part("detail");
            view! { <div test_id?=detail_id><text small>{detail}</text></div> }
        });
        let (caption, hint) = if icon.is_some() && !labelled {
            (None, Some(label))
        } else {
            (Some(label), None)
        };
        let icon = icon.map(|icon| {
            let icon_id = test_id.part("icon");
            view! { <div flex flex_none test_id?=icon_id>{Icon::new(icon, Role::TextSecondary)}</div> }
        });
        let badge = badge.map(|badge| {
            view! { <div absolute top=0.0 right=0.0 flex pass_through>{badge}</div> }
        });
        let height = by_touch(SEGMENT_TOUCH, 24.0);
        let root = view! {
            <div relative h=height flex items_center justify_center gap=Space::X1 px=padding rounded=tokens::Radius::Inner
                 text_color=Role::TextSecondary>
                {icon}
                {caption.map(|caption| view! { <text small>{caption}</text> })}
                {detail}
                {badge}
            </div>
        }
        .hover(|style| style.text_color(Role::Text))
        .selected(|style| style.text_color(Role::Text));
        let selected = Rc::new(selected);
        let root = match use_context::<SegmentIndicator>() {
            Some(SegmentIndicator(spot)) => {
                let on = selected.clone();
                spot.track(root, move || on.get())
            }
            None => root.selected(|style| style.bg(Role::SurfaceSelected)),
        }
        .is_selected(move || selected.get());
        let disabled = Rc::new(disabled);
        let root = fade_when_disabled(root, &disabled).enabled(move || !disabled.get());
        let root = activate(root, on_click, FocusPolicy::Tab);
        style
            .apply(root.when_some(test_id, Div::test_id))
            .hint(hint)
            .build(cx);
    }
}

const SEGMENT_TOUCH: f32 = 36.0;

pub struct Disclosure {
    expanded: Prop<bool>,
    on_toggle: Callback,
    body: Option<Rc<dyn Fn() -> AnyView>>,
    test_id: Option<String>,
    children: Vec<AnyView>,
}

#[component]
pub fn disclosure<T>(expanded: impl IntoProp<bool, T>) -> Disclosure {
    Disclosure {
        expanded: expanded.into_prop(),
        on_toggle: Callback::default(),
        body: None,
        test_id: None,
        children: Vec::new(),
    }
}

impl Disclosure {
    pub fn on_toggle(mut self, on_toggle: impl Into<Callback>) -> Self {
        self.on_toggle = on_toggle.into();
        self
    }

    pub fn body<T: View>(mut self, body: impl Fn() -> T + 'static) -> Self {
        self.body = Some(Rc::new(move || body().into_any()));
        self
    }

    pub fn accordion<T: Clone + PartialEq + 'static>(
        mut self,
        open: Signal<Option<T>>,
        key: T,
    ) -> Self {
        let held = key.clone();
        self.expanded = (move || open.with(|open| open.as_ref() == Some(&held))).into_prop();
        self.on_toggle = Callback::from(move || {
            open.update(|open| {
                *open = if open.as_ref() == Some(&key) {
                    None
                } else {
                    Some(key.clone())
                };
            });
        });
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.children.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.children
            .extend(children.into_iter().map(View::into_any));
        self
    }
}

impl View for Disclosure {
    fn build(self, cx: &mut BuildCx<'_>) {
        let expanded = Rc::new(self.expanded);
        let open = {
            let expanded = expanded.clone();
            move || expanded.get()
        };
        let header = view! {
            <div flex items_center gap=Space::X2 min_h=tokens::ICON_BUTTON_SIZE rounded=tokens::Radius::Inner border=1.0
                 border_color=Color::TRANSPARENT>
                {chevron(open.clone())}
                <div>{self.children}</div>
            </div>
        };
        let header = activate(focus_ring(header), self.on_toggle, FocusPolicy::Tab)
            .when_some(self.test_id, Div::test_id);
        let Some(body) = self.body else {
            return header.build(cx);
        };
        let hidden = Pose::new().opacity(0.0);
        let panel = view! {
            <show when=open enter=(hidden, Pace::Enter) exit=(hidden, Pace::Exit)>
                <div pl=DISCLOSURE_INDENT>{body()}</div>
            </show>
        };
        view! {
            <div flex col layout_transition=Pace::Layout>
                {header}
                {panel}
            </div>
        }
        .build(cx);
    }
}

const DISCLOSURE_INDENT: f32 = tokens::ICON_SIZE_SMALL + 8.0;

pub(crate) fn chevron<T>(expanded: impl IntoProp<bool, T>) -> Div {
    let expanded = expanded.into_prop();
    view! {
        <div flex_none
             rotate={move || if expanded.get() { 90.0 } else { 0.0 }} transition=Pace::Move>
            {Icon::new(IconName::ChevronRight, Role::TextMuted).size(tokens::ICON_SIZE_SMALL)}
        </div>
    }
}

#[component]
pub fn muted_text(#[prop(into)] children: String) -> Text {
    text(children).small().muted()
}

pub struct SectionHeader {
    title: String,
    expanded: Prop<bool>,
    detail: Option<String>,
    icon: Option<IconName>,
    badge: Option<AnyView>,
    action: Option<AnyView>,
    actions_always_visible: bool,
    on_toggle: Callback,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn section_header(#[prop(into)] title: String) -> SectionHeader {
    SectionHeader {
        title,
        expanded: Prop::Static(true),
        detail: None,
        icon: None,
        badge: None,
        action: None,
        actions_always_visible: false,
        on_toggle: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl SectionHeader {
    pub fn expanded<T>(mut self, expanded: impl IntoProp<bool, T>) -> Self {
        self.expanded = expanded.into_prop();
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn badge(mut self, badge: impl View) -> Self {
        self.badge = Some(badge.into_any());
        self
    }

    pub fn action(mut self, action: impl View) -> Self {
        self.action = Some(action.into_any());
        self
    }

    pub fn actions_always_visible(mut self) -> Self {
        self.actions_always_visible = true;
        self
    }

    pub fn on_toggle(mut self, on_toggle: impl Into<Callback>) -> Self {
        self.on_toggle = on_toggle.into();
        self
    }
}

chrome!(SectionHeader);

impl View for SectionHeader {
    fn build(self, cx: &mut BuildCx<'_>) {
        let group = format!("section-{}", self.title);
        let always = self.actions_always_visible;
        let action = self.action.map(|action| {
            let root = div().child(action);
            if always {
                root
            } else {
                root.invisible().group_hover(group.clone(), Styled::visible)
            }
        });
        let toggle = self.test_id.part("toggle");
        let heading = view! {
            <div flex items_center gap=Space::X2>
                {self.icon.map(|icon| Icon::new(icon, Role::TextMuted))}
                <text title primary>{self.title}</text>
                {self.detail.map(|detail| view! { <muted_text>{detail}</muted_text> })}
                {self.badge}
            </div>
        };
        let disclosure = disclosure(self.expanded)
            .on_toggle(self.on_toggle)
            .when_some(toggle, Disclosure::test_id)
            .child(heading);
        let root = view! {
            <div flex items_center justify_between px=Space::X2 h=tokens::GROUP_HEIGHT bg=Role::Chrome>
                {disclosure}
                {action}
            </div>
        }
        .group(group);
        self.style
            .apply(root.when_some(self.test_id, Div::test_id))
            .build(cx);
    }
}

#[derive(Clone)]
pub struct RowAction {
    pub(crate) label: String,
    pub(crate) icon: Option<IconName>,
    icon_only: bool,
    pub(crate) destructive: bool,
    pub(crate) on_select: Callback,
    pub(crate) test_id: Option<String>,
}

pub fn row_action(label: impl Into<String>, on_select: impl Into<Callback>) -> RowAction {
    RowAction {
        label: label.into(),
        icon: None,
        icon_only: false,
        destructive: false,
        on_select: on_select.into(),
        test_id: None,
    }
}

impl RowAction {
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn icon_only(mut self) -> Self {
        self.icon_only = true;
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }

    fn cell(
        self,
        ink: Role,
        labelled: bool,
        chosen: impl Fn() + 'static,
        focused: impl Fn(bool) + Clone + 'static,
    ) -> Div {
        let (on_select, blurred) = (self.on_select, focused.clone());
        let icon = self.icon.map(|icon| Icon::new(icon, ink));
        let label = labelled.then(|| view! { <text label no_wrap>{self.label.clone()}</text> });
        let cell = view! {
            <div flex flex_none items_center justify_center gap=Space::X1 border=1.0
                 border_color=Color::TRANSPARENT text_color=ink test_id?=self.test_id
                 on_focus={move |_| focused(true)} on_blur={move |_| blurred(false)}>
                {icon}
                {label}
            </div>
        };
        focus_ring(cell).on_activate(move || {
            on_select.call();
            chosen();
        })
    }

    fn chip(self, focused: impl Fn(bool) + Clone + 'static) -> AnyView {
        let ink = if self.destructive {
            Role::Destructive
        } else {
            Role::Text
        };
        let square = self.icon_only && self.icon.is_some();
        let label = self.label.clone();
        let chip = self
            .cell(ink, !square, || {}, focused)
            .h(tokens::ICON_BUTTON_SIZE)
            .rounded(tokens::Radius::Inner)
            .hover(|style| style.bg(Role::SurfaceHover));
        if square {
            chip.w(tokens::ICON_BUTTON_SIZE).tooltip(label).into_any()
        } else {
            chip.px(Space::X2).into_any()
        }
    }

    fn block(self, chosen: impl Fn() + 'static, focused: impl Fn(bool) + Clone + 'static) -> Div {
        let (fill, ink) = if self.destructive {
            (Role::Destructive, Role::DestructiveForeground)
        } else {
            (Role::Accent, Role::AccentForeground)
        };
        self.cell(ink, true, chosen, focused).px(Space::X4).bg(fill)
    }
}

pub struct ListRow {
    depth: usize,
    selection: Prop<SelectionState>,
    on_click: Callback,
    actions: Vec<RowAction>,
    leading: Option<AnyView>,
    detail: Option<String>,
    trailing: Option<AnyView>,
    chevron: bool,
    test_id: Option<String>,
    style: StyleDelta,
    children: Vec<AnyView>,
}

#[derive(Clone, Copy)]
pub(crate) struct Grouped;

#[component]
pub fn list_row() -> ListRow {
    ListRow {
        depth: 0,
        selection: Prop::Static(SelectionState::None),
        on_click: Callback::default(),
        actions: Vec::new(),
        leading: None,
        detail: None,
        trailing: None,
        chevron: false,
        test_id: None,
        style: StyleDelta::default(),
        children: Vec::new(),
    }
}

impl ListRow {
    pub fn action(mut self, action: RowAction) -> Self {
        self.actions.push(action);
        self
    }

    /// Shown at the row's trailing edge, and gives way to the actions while they show.
    pub fn trailing(mut self, trailing: impl View) -> Self {
        self.trailing = Some(trailing.into_any());
        self
    }

    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn selection<T>(mut self, selection: impl IntoProp<SelectionState, T>) -> Self {
        self.selection = selection.into_prop();
        self
    }

    pub fn on_click(mut self, on_click: impl Into<Callback>) -> Self {
        self.on_click = on_click.into();
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.children.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.children
            .extend(children.into_iter().map(View::into_any));
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn chevron(mut self) -> Self {
        self.chevron = true;
        self
    }

    pub fn leading(mut self, leading: impl View) -> Self {
        self.leading = Some(leading.into_any());
        self
    }
}

chrome!(ListRow);

impl View for ListRow {
    fn build(self, cx: &mut BuildCx<'_>) {
        let selection = Rc::new(self.selection.memo());
        let look = {
            let selection = selection.clone();
            move || Theme::selection_style(selection.get())
        };
        let shown = {
            let selection = selection.clone();
            move || {
                matches!(
                    selection.get(),
                    SelectionState::Selected | SelectionState::Focused
                )
            }
        };
        let background = {
            let look = look.clone();
            move || look().background.unwrap_or_default()
        };
        let grouped = use_context::<Grouped>().is_some();
        let (height, taller) = (row_height(), self.detail.is_some());
        let min_h = move || height() + if taller { DETAIL_HEIGHT } else { 0.0 };
        let (inset, gap) = if grouped {
            (Space::X3.px(), Space::X3)
        } else {
            (8.0, Space::X2)
        };
        let body = match self.detail {
            Some(detail) => vec![
                view! {
                    <div flex col flex_1 min_w=0.0>
                        <div flex items_center gap=Space::X2>{self.children}</div>
                        <text small muted no_wrap>{detail}</text>
                    </div>
                }
                .into_any(),
            ],
            None => self.children,
        };
        let pointer = self.chevron.then(|| {
            Icon::new(IconName::ChevronRight, Role::TextMuted)
                .size(tokens::ICON_SIZE_SMALL)
                .into_any()
        });
        let trailing = match (self.trailing, pointer) {
            (Some(trailing), Some(pointer)) => Some(
                view! { <div flex items_center gap=Space::X1>{trailing}{pointer}</div> }.into_any(),
            ),
            (trailing, pointer) => trailing.or(pointer),
        };
        let root = view! {
            <div flex items_center min_h=min_h pl={inset + indent(self.depth)} pr={inset} gap=gap
                 rounded=tokens::Radius::Inner text_color={move || look().foreground}>
                {self.leading}
                {body}
            </div>
        }
        .hover(|style| style.bg(Role::SurfaceHover))
        .selected(|style| style.bg(background.clone()))
        .is_selected(shown.clone())
        .enabled(move || selection.get() != SelectionState::Disabled)
        .when_some(self.test_id, Div::test_id);
        let resting = |trailing: AnyView| view! { <div flex flex_none items_center ml=auto()>{trailing}</div> };
        let root = if self.actions.is_empty() {
            activate(
                root.children(trailing.map(resting)),
                self.on_click,
                FocusPolicy::Tab,
            )
        } else if presentation().get_untracked().is_touch() {
            swiped(
                root.children(trailing.map(resting)),
                self.actions,
                self.on_click,
            )
        } else {
            let fill = move || {
                if shown() {
                    background()
                } else {
                    Fill::Role(Role::SurfaceHover)
                }
            };
            hovered(root, trailing, fill, self.actions, self.on_click)
        };
        self.style.apply(root).build(cx);
    }
}

fn hovered(
    root: Div,
    trailing: Option<AnyView>,
    fill: impl Fn() -> Fill + 'static,
    actions: Vec<RowAction>,
    on_click: Callback,
) -> Div {
    let (over, within) = (signal(false), signal(0u32));
    let focused = move |on: bool| {
        within.update(|count| *count = count.saturating_add_signed(if on { 1 } else { -1 }));
    };
    let revealed = move || over.get() || within.get() > 0;
    let chips = actions.into_iter().map(|action| action.chip(focused));
    let tray = view! {
        <div flex flex_none items_center gap=Space::X1 opacity={move || if revealed() { 1.0 } else { 0.0 }}
             transition=Pace::Move>
            {..chips}
        </div>
    };
    let edge = match trailing {
        Some(trailing) => view! {
            <div relative flex flex_none items_center justify_end ml=auto()>
                <div flex items_center opacity={move || if revealed() { 0.0 } else { 1.0 }} transition=Pace::Move>
                    {trailing}
                </div>
                {tray.absolute().right(0.0).top(0.0).bottom(0.0).pl(Space::X1).bg(fill)}
            </div>
        },
        None => tray.ml(auto()),
    };
    activate(root, on_click, FocusPolicy::Tab)
        .on_hover(move |inside| over.set(inside))
        .child(edge)
}

fn swiped(root: Div, actions: Vec<RowAction>, on_click: Callback) -> Div {
    let (x, origin, tray) = (motion(0.0), signal(0.0), signal(Size::ZERO));
    let rest = move |open: bool| {
        let to = if open {
            -tray.get_untracked().width
        } else {
            0.0
        };
        x.animate_to(to, Pace::Gesture);
    };
    let blocks = actions
        .into_iter()
        .map(|action| action.block(move || rest(false), rest));
    let strip = view! {
        <div absolute left=pct(100.0) top=0.0 bottom=0.0 flex track_size=tray>{..blocks}</div>
    };
    let row = root
        .relative()
        .translate_x(x)
        .drag_axis(DragAxis::Horizontal)
        .on_drag(move |event| {
            let width = tray.get_untracked().width;
            match event.phase {
                DragPhase::Start => origin.set(x.get()),
                DragPhase::Move => {
                    let to = origin.get_untracked() + event.translation.x;
                    let past = (-width - to).max(0.0);
                    x.set(to.clamp(-width, 0.0) - past * SWIPE_RESISTANCE);
                }
                DragPhase::End => {
                    let projected = x.get() + event.velocity.x * SWIPE_PROJECTION;
                    let to = if projected < -width / 2.0 {
                        -width
                    } else {
                        0.0
                    };
                    x.fling_to(to, event.velocity.x, Timing::from(Pace::Gesture));
                }
            }
        })
        .on_activate(move || {
            if x.get() < 0.0 {
                rest(false);
            } else {
                on_click.call();
            }
        })
        .child(strip);
    view! { <div relative overflow_hidden>{row}</div> }
}

const DETAIL_HEIGHT: f32 = 12.0;
const SWIPE_RESISTANCE: f32 = 0.2;
const SWIPE_PROJECTION: f32 = 0.2;

pub struct Backdrop {
    align_top: bool,
    anchor_below: Option<Signal<Rect>>,
    on_dismiss: Callback,
    test_id: Option<String>,
    children: Vec<AnyView>,
}

#[component]
pub fn backdrop() -> Backdrop {
    Backdrop {
        align_top: false,
        anchor_below: None,
        on_dismiss: Callback::default(),
        test_id: None,
        children: Vec::new(),
    }
}

impl Backdrop {
    pub fn align_top(mut self) -> Self {
        self.align_top = true;
        self
    }

    pub fn anchor_below(mut self, anchor: Signal<Rect>) -> Self {
        self.anchor_below = Some(anchor);
        self
    }

    pub fn on_dismiss(mut self, on_dismiss: impl Into<Callback>) -> Self {
        self.on_dismiss = on_dismiss.into();
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.children.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.children
            .extend(children.into_iter().map(View::into_any));
        self
    }
}

impl View for Backdrop {
    fn build(self, cx: &mut BuildCx<'_>) {
        let Backdrop {
            align_top,
            anchor_below,
            on_dismiss,
            test_id,
            children,
        } = self;
        let children = Cell::new(Some(children));
        let scrim = move || {
            let inner = view! {
                <div flex col min_h=0.0 max_h=pct(100.0) max_w=pct(100.0)>{children.take()}</div>
            };
            let root = view! {
                <div flex col focus_scope autofocus tab_index=-1 test_id?={test_id.clone()}
                     on_key_down={move |event| {
                         if event.key != Key::Tab {
                             event.stop_propagation();
                         }
                     }}/>
            };
            let (root, inner) = if let Some(anchor) = anchor_below {
                (root, Placement::Below.place(inner, anchor))
            } else {
                let root = root
                    .items_center()
                    .p(Space::X4)
                    .bg(Role::Scrim)
                    .backdrop(BackdropBlur::new(SCRIM_BLUR));
                let root = if align_top {
                    root.justify_start().pt(tokens::MODAL_TOP_OFFSET)
                } else {
                    root.justify_center()
                };
                (root, inner)
            };
            let inner = Cell::new(Some(inner.on_pointer_down(PointerEvent::stop_propagation)));
            dismiss_layer(root, move || on_dismiss.call()).child(view! {
                <show when={|| true} enter=(POPPED, Pace::Enter)>{inner.take()}</show>
            })
        };
        view! {
            <overlay>
                <show when={|| true} children=scrim enter=(Pose::new().opacity(0.0), Pace::Enter)/>
            </overlay>
        }
        .build(cx);
    }
}

const SCRIM_BLUR: f32 = 8.0;

fn modal_width(size: ModalSize) -> Length {
    px(match size {
        ModalSize::Compact => tokens::MODAL_WIDTH_COMPACT,
        ModalSize::Medium => tokens::MODAL_WIDTH,
        ModalSize::Wide => tokens::MODAL_WIDTH_WIDE,
    })
}

pub struct Modal {
    title: String,
    hint: Option<String>,
    size: ModalSize,
    height: Option<Length>,
    align_top: bool,
    on_dismiss: Callback,
    test_id: Option<String>,
    children: Vec<AnyView>,
}

#[component]
pub fn modal(#[prop(into)] title: String) -> Modal {
    Modal {
        title,
        hint: None,
        size: ModalSize::default(),
        height: None,
        align_top: false,
        on_dismiss: Callback::default(),
        test_id: None,
        children: Vec::new(),
    }
}

impl Modal {
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn modal_size(mut self, size: ModalSize) -> Self {
        self.size = size;
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = Some(height.into());
        self
    }

    pub fn align_top(mut self) -> Self {
        self.align_top = true;
        self
    }

    pub fn on_dismiss(mut self, on_dismiss: impl Into<Callback>) -> Self {
        self.on_dismiss = on_dismiss.into();
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.children.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.children
            .extend(children.into_iter().map(View::into_any));
        self
    }
}

impl View for Modal {
    fn build(self, cx: &mut BuildCx<'_>) {
        if presentation().get_untracked().is_handheld() {
            let parts = Cell::new(Some((self.title, self.hint, self.children)));
            let presenter = Presenter {
                detents: vec![Detent::Large],
                test_id: self.test_id,
                on_closed: self.on_dismiss,
                ..Presenter::new(signal(true))
            };
            let content = move || {
                let (title, hint, children) = parts.take().unwrap_or_default();
                view! {
                    <div flex col flex_1 min_h=0.0 gap=Space::X2 px=Space::X4 pb=Space::X4>
                        <text heading primary>{title}</text>
                        {hint.map(|hint| view! { <muted_text>{hint}</muted_text> })}
                        <div flex col flex_1 min_h=0.0>{children}</div>
                    </div>
                }
                .into_any()
            };
            presenter.present(content).build(cx);
            return;
        }
        let backdrop_id = self.test_id.part("backdrop");
        let panel = view! {
            <div test_id?=self.test_id flex col gap=Space::X2 p=Space::X4 w={modal_width(self.size)}
                 max_w=pct(100.0) h?=self.height overflow_hidden material=Material::Glass>
                <text heading primary>{self.title}</text>
                {self.hint.map(|hint| view! { <muted_text>{hint}</muted_text> })}
                <div flex col min_h=0.0>{self.children}</div>
            </div>
        };
        let backdrop = backdrop()
            .on_dismiss(self.on_dismiss)
            .when_some(backdrop_id, Backdrop::test_id)
            .child(panel);
        if self.align_top {
            backdrop.align_top()
        } else {
            backdrop
        }
        .build(cx);
    }
}

#[component]
pub fn empty_state(
    icon: IconName,
    #[prop(into)] title: String,
    #[prop(into)] detail: String,
) -> Div {
    view! {
        <div flex col items_center justify_center p=Space::X4 gap=Space::X2>
            <div flex flex_none items_center justify_center size=tokens::BAR_HEIGHT material=Material::GlassControl>
                {Icon::new(icon, Role::TextSecondary)}
            </div>
            <text title primary>{title}</text>
            <muted_text>{detail}</muted_text>
        </div>
    }
}

#[component]
pub fn notification(#[prop(into)] message: String, tone: NoticeTone, title: Option<String>) -> Div {
    let fg = move || Theme::notice_style(tone).foreground;
    let titled = title.is_some();
    view! {
        <div flex gap=Space::X2 p=Space::X2 rounded=tokens::Radius::Control
             when:items_start=titled when:items_center={!titled}
             bg={move || Theme::notice_style(tone).background.unwrap_or_default()}>
            {Icon::new(tone_icon(tone), fg)}
            <div flex col flex_1 min_w=0.0>
                {title.map(|title| view! { <text label color=fg>{title}</text> })}
                <text body color=fg>{message}</text>
            </div>
        </div>
    }
}

pub(crate) fn activate(root: Div, action: Callback, focus: FocusPolicy) -> Div {
    if action.is_set() {
        root.on_activate(move || action.call()).focus_policy(focus)
    } else {
        root
    }
}

use crate::{
    icon::{Icon, IconName},
    material::{Material, MaterialExt},
    popover::{Hint, Placement, dismissable},
    tokens,
    tree::indent,
    typography::UiStyles,
    util::fade_when_disabled,
};
use rouge_core::Role;
use rouge_core::{
    AnyView, Apply, BuildCx, Color, Div, Fill, FocusPolicy, IntoProp, Key, LayoutTransition,
    Length, Path, Prop, Rect, Spring, StyleDelta, Styled, Text, Theme, View, canvas, component,
    div, dynamic, overlay, pct, px, styled, text, view,
};
use rouge_reactive::{Callback, Signal, effect, memo, signal, use_context};
use rouge_theme::{
    ButtonVariant, ControlState, InteractionState, ModalSize, NoticeTone, SelectionState,
};
use std::cell::Cell;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlSize {
    Small,
    #[default]
    Medium,
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
    size: ControlSize,
    tooltip: Option<String>,
    disabled: Prop<bool>,
    selected: Prop<bool>,
    focus: FocusPolicy,
    on_click: Callback,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn button<U>(label: impl IntoProp<String, U>) -> Button {
    Button {
        label: label.into_prop(),
        icon: None,
        variant: ButtonVariant::default(),
        size: ControlSize::default(),
        tooltip: None,
        disabled: Prop::Static(false),
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
        self.size = size;
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

    pub fn selected<U>(mut self, selected: impl IntoProp<bool, U>) -> Self {
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

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }
}

styled!(Button => style);

impl View for Button {
    fn build(self, cx: &mut BuildCx<'_>) {
        let variant = self.variant;
        let look = move |interaction, selected| {
            move || Theme::control_style(variant, ControlState::new(interaction).selected(selected))
        };
        let fill = move |interaction, selected| {
            let look = look(interaction, selected);
            move || look().background.unwrap_or_default()
        };
        let ink = move |interaction, selected| {
            let look = look(interaction, selected);
            move || look().foreground
        };
        let (height, padding) = match self.size {
            ControlSize::Small => (24.0, tokens::CONTROL_PADDING_X_SMALL),
            ControlSize::Medium => (tokens::ROW_HEIGHT, tokens::CONTROL_PADDING_X_MEDIUM),
        };
        let disabled = Rc::new(self.disabled);
        let selected = Rc::new(self.selected);
        let hovered = signal(false);
        let icon = self.icon.map(|icon| {
            let (disabled, selected) = (disabled.clone(), selected.clone());
            Icon::new(icon, move || {
                let interaction = if disabled.get() {
                    InteractionState::Disabled
                } else if hovered.get() {
                    InteractionState::Hovered
                } else {
                    InteractionState::Rest
                };
                look(interaction, selected.get())().foreground
            })
        });
        let root = view! {
            <div flex items_center justify_center flex_none gap=8.0 rounded=tokens::Radius::Control
                 border=1.0 border_color=Color::TRANSPARENT h=height px=padding
                 font_weight=500.0 bg={fill(InteractionState::Rest, false)}
                 text_color={ink(InteractionState::Rest, false)}
                 on_hover={move |inside| hovered.set(inside)}>
                {icon}
                <text label no_wrap>{self.label}</text>
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
        .focus_visible(|style| style.border_color(Role::Accent))
        .disabled(|style| {
            style
                .bg(fill(InteractionState::Disabled, false))
                .text_color(ink(InteractionState::Disabled, false))
        })
        .is_selected({
            let selected = selected.clone();
            move || selected.get()
        })
        .enabled(move || !disabled.get());
        let root = activate(root, self.on_click, self.focus);
        self.style
            .apply(root.when_some(self.test_id, Div::test_id))
            .hint(self.tooltip)
            .build(cx);
    }
}

pub struct IconButton {
    icon: IconName,
    label: String,
    icon_size: f32,
    color: Option<Prop<Fill>>,
    disabled: Prop<bool>,
    selected: Prop<bool>,
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
        icon_size: tokens::ICON_SIZE,
        color: None,
        disabled: Prop::Static(false),
        selected: Prop::Static(false),
        focus: FocusPolicy::Tab,
        on_click: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl IconButton {
    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = size;
        self
    }

    pub fn color<T>(mut self, color: impl IntoProp<Fill, T>) -> Self {
        self.color = Some(color.into_prop());
        self
    }

    pub fn disabled<U>(mut self, disabled: impl IntoProp<bool, U>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }

    pub fn selected<U>(mut self, selected: impl IntoProp<bool, U>) -> Self {
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

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }
}

styled!(IconButton => style);

impl View for IconButton {
    fn build(self, cx: &mut BuildCx<'_>) {
        let disabled = Rc::new(self.disabled);
        let selected = Rc::new(self.selected);
        let hovered = signal(false);
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
        let root = view! {
            <div flex flex_none items_center justify_center size=tokens::ICON_BUTTON_SIZE
                 rounded=tokens::Radius::Control on_hover={move |inside| hovered.set(inside)}>
                {Icon::new(self.icon, foreground).size(self.icon_size)}
            </div>
        }
        .hover(|style| style.bg(Role::SurfaceHover))
        .selected(|style| style.bg(Role::SurfaceSelected))
        .focus_visible(|style| style.bg(Role::SurfaceSelected))
        .disabled(|style| style.bg(Color::TRANSPARENT))
        .is_selected(move || selected.get());
        let root = fade_when_disabled(root, &disabled).enabled(move || !disabled.get());
        let root = activate(root, self.on_click, self.focus);
        self.style
            .apply(root.when_some(self.test_id, Div::test_id))
            .tooltip(self.label)
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

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }
}

styled!(Checkbox<T> => style);

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
            let color = mark_color.unwrap_or_else(|| theme.get().text);
            painter.stroke_round(&path, (size.width / 6.0).min(2.0), color);
        })
        .size_full();
        let disabled = Rc::new(self.disabled);
        let root = view! {
            <div flex_none size=tokens::CHECKBOX_SIZE rounded=tokens::Radius::Inner border=1.0 bg=Role::Canvas
                 border_color=Role::TextMuted>
                {mark}
            </div>
        }
        .hover(|style| style.border_color(Role::Accent))
        .focus_visible(|style| style.border_color(Role::Accent));
        let root = fade_when_disabled(root, &disabled)
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

#[derive(Clone, Copy)]
struct SegmentIndicator(Signal<Option<(u64, Rect)>>);

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
            <div relative flex items_center gap=4.0 p=4.0 rounded=tokens::Radius::Control bg=Role::Chrome/>
        },
        indicator: StyleDelta::default()
            .rounded(tokens::Radius::Inner)
            .bg(Role::SurfaceSelected),
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

styled!(SegmentedControl => root);

impl View for SegmentedControl {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (control, selected) = (signal(Rect::ZERO), signal(None::<(u64, Rect)>));
        let placed = signal(None::<Rect>);
        effect(move || {
            if let Some((_, at)) = selected.get() {
                let origin = control.get().origin;
                let at = Rect::new(
                    at.origin.x - origin.x,
                    at.origin.y - origin.y,
                    at.size.width,
                    at.size.height,
                );
                placed.set_if_changed(Some(at));
            }
        });
        let ready = memo(move || placed.with(Option::is_some));
        let indicator = Cell::new(Some((
            self.indicator,
            self.test_id.as_ref().map(|id| format!("{id}-indicator")),
        )));
        let indicator = dynamic(move || {
            let (style, test_id) = ready.get().then(|| indicator.take()).flatten()?;
            let geometry = move |f: fn(Rect) -> f32| move || placed.get().map_or(0.0, f);
            let pill = view! {
                <div absolute left={geometry(|at| at.origin.x)} top={geometry(|at| at.origin.y)}
                     w={geometry(|at| at.size.width)} h={geometry(|at| at.size.height)}
                     layout_transition={LayoutTransition::new(Spring::snappy()).size()}/>
            }
            .when(move || selected.with(Option::is_none), Styled::invisible)
            .when_some(test_id, Div::test_id);
            Some(style.apply(pill))
        });
        self.root
            .when_some(self.test_id, Div::test_id)
            .track_bounds(control)
            .provide(SegmentIndicator(selected))
            .child(indicator)
            .children(self.segments)
            .build(cx);
    }
}

pub struct Segment {
    label: String,
    icon: Option<IconName>,
    detail: Option<String>,
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

    pub fn selected<U>(mut self, selected: impl IntoProp<bool, U>) -> Self {
        self.selected = selected.into_prop();
        self
    }

    pub fn disabled<U>(mut self, disabled: impl IntoProp<bool, U>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }

    pub fn on_click(mut self, on_click: impl Into<Callback>) -> Self {
        self.on_click = on_click.into();
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }
}

styled!(Segment => style);

impl View for Segment {
    fn build(self, cx: &mut BuildCx<'_>) {
        let Segment {
            label,
            icon,
            detail,
            selected,
            disabled,
            on_click,
            test_id,
            style,
        } = self;
        let padding = if icon.is_some() {
            tokens::CONTROL_PADDING_X_SMALL
        } else {
            tokens::CONTROL_PADDING_X_MEDIUM
        };
        let detail = detail.filter(|_| icon.is_some()).map(|detail| {
            let detail_id = test_id.as_ref().map(|id| format!("{id}-detail"));
            view! { <div test_id?=detail_id><text small>{detail}</text></div> }
        });
        let (caption, hint) = if icon.is_some() {
            (None, Some(label))
        } else {
            (Some(label), None)
        };
        let root = view! {
            <div h=24.0 flex items_center justify_center gap=4.0 px=padding rounded=tokens::Radius::Inner
                 text_color=Role::TextMuted>
                {icon.map(|icon| Icon::new(icon, Role::TextMuted))}
                {detail}
                {caption.map(|caption| view! { <text small>{caption}</text> })}
            </div>
        }
        .hover(|style| style.text_color(Role::Text))
        .selected(|style| style.text_color(Role::Text));
        let selected = Rc::new(selected);
        let root = match use_context::<SegmentIndicator>() {
            Some(SegmentIndicator(indicator)) => {
                let bounds = signal(Rect::ZERO);
                let (key, on) = (bounds.id(), selected.clone());
                effect(move || {
                    let at = bounds.get();
                    if on.get() && at.size.width > 0.0 {
                        indicator.set_if_changed(Some((key, at)));
                    } else if indicator
                        .with_untracked(|held| held.is_some_and(|(held, _)| held == key))
                    {
                        indicator.set(None);
                    }
                });
                root.track_bounds(bounds)
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

pub struct Disclosure {
    expanded: Prop<bool>,
    on_toggle: Callback,
    test_id: Option<String>,
    children: Vec<AnyView>,
}

#[component]
pub fn disclosure<U>(expanded: impl IntoProp<bool, U>) -> Disclosure {
    Disclosure {
        expanded: expanded.into_prop(),
        on_toggle: Callback::default(),
        test_id: None,
        children: Vec::new(),
    }
}

impl Disclosure {
    pub fn on_toggle(mut self, on_toggle: impl Into<Callback>) -> Self {
        self.on_toggle = on_toggle.into();
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
        let root = view! {
            <div flex items_center gap=8.0>
                {chevron(self.expanded)}
                <div>{self.children}</div>
            </div>
        };
        activate(root, self.on_toggle, FocusPolicy::Tab)
            .when_some(self.test_id, Div::test_id)
            .build(cx);
    }
}

pub(crate) fn chevron<U>(expanded: impl IntoProp<bool, U>) -> Div {
    let expanded = expanded.into_prop();
    div()
        .flex_none()
        .child(Icon::new(IconName::ChevronRight, Role::TextMuted).size(tokens::ICON_SIZE_SMALL))
        .rotate(move || if expanded.get() { 90.0 } else { 0.0 })
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
    pub fn expanded<U>(mut self, expanded: impl IntoProp<bool, U>) -> Self {
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

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }
}

styled!(SectionHeader => style);

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
        let toggle = self.test_id.as_ref().map(|id| format!("{id}-toggle"));
        let heading = view! {
            <div flex items_center gap=8.0>
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
            <div flex items_center justify_between px=8.0 h=tokens::GROUP_HEIGHT bg=Role::Chrome>
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

pub struct ListRow {
    depth: usize,
    selection: Prop<SelectionState>,
    on_click: Callback,
    test_id: Option<String>,
    style: StyleDelta,
    children: Vec<AnyView>,
}

#[component]
pub fn list_row() -> ListRow {
    ListRow {
        depth: 0,
        selection: Prop::Static(SelectionState::None),
        on_click: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
        children: Vec::new(),
    }
}

impl ListRow {
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn selection<U>(mut self, selection: impl IntoProp<SelectionState, U>) -> Self {
        self.selection = selection.into_prop();
        self
    }

    pub fn on_click(mut self, on_click: impl Into<Callback>) -> Self {
        self.on_click = on_click.into();
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

styled!(ListRow => style);

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
        let root = view! {
            <div flex items_center min_h=tokens::ROW_HEIGHT pl={8.0 + indent(self.depth)} pr=8.0 gap=8.0
                 rounded=tokens::Radius::Inner text_color={move || look().foreground}>
                {self.children}
            </div>
        }
        .hover(|style| style.bg(Role::SurfaceHover))
        .selected(|style| style.bg(background))
        .is_selected(shown)
        .enabled(move || selection.get() != SelectionState::Disabled);
        let root = activate(root, self.on_click, FocusPolicy::Tab);
        self.style
            .apply(root.when_some(self.test_id, Div::test_id))
            .build(cx);
    }
}

#[component]
pub fn action_bar() -> Div {
    view! {
        <div flex items_center justify_end gap=8.0 min_h=tokens::BAR_HEIGHT px=12.0 bg=Role::Chrome/>
    }
}

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
        overlay(move || {
            let inner = view! {
                <div flex col min_h=0.0 max_h=pct(100.0) max_w=pct(100.0)>{children}</div>
            };
            let root = view! {
                <div flex col focus_scope autofocus tab_index=-1 test_id?=test_id
                     on_key_down={move |event| {
                         if event.key != Key::Tab {
                             event.stop_propagation();
                         }
                     }}/>
            };
            let (root, inner) = if let Some(anchor) = anchor_below {
                (root, Placement::Below.place(inner, anchor))
            } else {
                let root = root.items_center().p(16.0).bg(Role::Scrim);
                let root = if align_top {
                    root.justify_start().pt(tokens::MODAL_TOP_OFFSET)
                } else {
                    root.justify_center()
                };
                (root, inner)
            };
            dismissable(root, inner, move || on_dismiss.call())
        })
        .build(cx);
    }
}

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
        let backdrop_id = self.test_id.as_ref().map(|id| format!("{id}-backdrop"));
        let panel = view! {
            <div test_id?=self.test_id flex col gap=8.0 p=16.0 w={modal_width(self.size)}
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
        <div flex col items_center justify_center p=16.0 gap=8.0>
            {Icon::new(icon, Role::TextMuted)}
            <text title primary>{title}</text>
            <muted_text>{detail}</muted_text>
        </div>
    }
}

#[component]
pub fn notification(#[prop(into)] message: String, tone: NoticeTone) -> Div {
    let fg = move || Theme::notice_style(tone).foreground;
    let icon = match tone {
        NoticeTone::Error | NoticeTone::Warning => IconName::TriangleAlert,
        NoticeTone::Positive => IconName::CircleCheck,
        _ => IconName::Info,
    };
    view! {
        <div flex items_center gap=8.0 p=8.0 rounded=tokens::Radius::Control
             bg={move || Theme::notice_style(tone).background.unwrap_or_default()}>
            {Icon::new(icon, fg)}
            <text body color=fg>{message}</text>
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

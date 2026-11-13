use crate::{
    icon::{Icon, IconName},
    material::{Material, MaterialExt},
    popover::{Hint, Placement, dismissable},
    tokens,
    tree::indent,
    typography::UiStyles,
    util::{interaction, pressable},
};
use rouge_core::{
    AnyView, Color, Div, Interactions, Key, Length, Path, Prop, Rect, Styled, Text, Theme, View,
    canvas, component, div, overlay, pct, px, text, view,
};
use rouge_reactive::{Callback, Signal, memo, signal};
use rouge_theme::{
    ButtonVariant, CheckState, ControlSize, ControlState, ModalSize, NoticeTone, SelectionState,
};

#[component]
pub fn button(
    #[prop(into)] id: String,
    #[prop(reactive)] label: Prop<String>,
    icon: Option<IconName>,
    #[prop(default)] variant: ButtonVariant,
    #[prop(default)] size: ControlSize,
    #[prop(into)] tooltip: Option<String>,
    #[prop(default)] disabled: bool,
    #[prop(default)] selected: bool,
    #[prop(default)] keyboard_focusable: bool,
    #[prop(default)] full_width: bool,
    #[prop(default, into)] on_click: Callback,
) -> AnyView {
    let theme = Theme::current();
    let state = signal(Interactions::default());
    let style = memo(move || {
        let interaction = interaction(state.get(), !disabled);
        theme.control_style(variant, ControlState::new(interaction).selected(selected))
    });
    let small = size == ControlSize::Small;
    let height = if small { 24.0 } else { tokens::ROW_HEIGHT };
    let padding = if small {
        tokens::CONTROL_PADDING_X_SMALL
    } else {
        tokens::CONTROL_PADDING_X_MEDIUM
    };
    let foreground = move || style.get().foreground;
    let root = view! {
        <div test_id=id flex items_center justify_center flex_none gap=6.0 rounded=tokens::RADIUS_CONTROL
             track_interaction=state border=px(1.0) h=height px=padding when:w_full=full_width
             border_color={move || if state.get().focus_visible() { theme.accent.get() } else { Color::TRANSPARENT }}
             bg={move || style.get().background.unwrap_or(Color::TRANSPARENT)}>
            {icon.map(|icon| Icon::new(icon, foreground))}
            <text label no_wrap weight={move || if style.get().emphasized { 600.0 } else { 500.0 }} color=foreground>
                {move || label.get()}
            </text>
        </div>
    };
    pressable(root, keyboard_focusable, !disabled, on_click).hint(tooltip)
}

#[component]
pub fn icon_button(
    #[prop(into)] id: String,
    icon: IconName,
    #[prop(into)] label: String,
    icon_size: Option<f32>,
    #[prop(default)] disabled: bool,
    #[prop(default)] selected: bool,
    #[prop(default)] keyboard_focusable: bool,
    #[prop(default, into)] on_click: Callback,
) -> AnyView {
    let theme = Theme::current();
    let enabled = !disabled;
    let state = signal(Interactions::default());
    let foreground = move || {
        if selected || (enabled && state.get().hovered) {
            theme.text.get()
        } else {
            theme.text_muted.get()
        }
    };
    let root = view! {
        <div test_id=id flex flex_none items_center justify_center size=tokens::ICON_BUTTON_SIZE
             rounded={tokens::ICON_BUTTON_SIZE / 2.0} track_interaction=state when:opacity=(disabled, tokens::DISABLED_OPACITY)
             bg={move || {
                 let state = state.get();
                 if selected || state.focus_visible() {
                     theme.surface_selected.get()
                 } else if enabled && state.hovered {
                     theme.surface_hover.get()
                 } else {
                     Color::TRANSPARENT
                 }
             }}>
            {Icon::new(icon, foreground).size(icon_size.unwrap_or(tokens::ICON_SIZE))}
        </div>
    };
    pressable(root, keyboard_focusable, enabled, on_click).hint(enabled.then_some(label))
}

#[component]
pub fn checkbox(
    #[prop(into)] id: String,
    #[prop(reactive)] state: Prop<CheckState>,
    #[prop(into)] tooltip: Option<String>,
    #[prop(default)] disabled: bool,
    #[prop(default, into)] on_toggle: Callback,
) -> AnyView {
    let theme = Theme::current();
    let enabled = !disabled;
    let interaction = signal(Interactions::default());
    let mark = canvas(move |painter| {
        let size = painter.size();
        let at = |x: f32, y: f32| (size.width * x, size.height * y);
        let path = match state.get() {
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
        painter.stroke_round(&path, size.width / 6.0, theme.text.get());
    })
    .size_full();
    let root = view! {
        <div test_id=id flex_none size=tokens::CHECKBOX_SIZE rounded=2.0 border=px(1.0) bg=theme.canvas
             track_interaction=interaction when:opacity=(disabled, tokens::DISABLED_OPACITY)
             border_color={move || {
                 let state = interaction.get();
                 if enabled && (state.hovered || state.focus_visible()) {
                     theme.accent.get()
                 } else {
                     theme.text_muted.get()
                 }
             }}>
            {mark}
        </div>
    };
    pressable(root, true, enabled, on_toggle).hint(tooltip.filter(|_| enabled))
}

#[component]
pub fn segmented_control(#[prop(into)] id: String) -> Div {
    view! {
        <div test_id=id flex items_center gap=2.0 p=2.0 rounded=tokens::RADIUS_CONTROL bg=Theme::current().chrome/>
    }
}

#[component]
pub fn segment(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    icon: Option<IconName>,
    #[prop(into)] detail: Option<String>,
    #[prop(default)] selected: bool,
    #[prop(default)] disabled: bool,
    #[prop(default, into)] on_click: Callback,
) -> AnyView {
    let theme = Theme::current();
    let interaction = signal(Interactions::default());
    let foreground = move || {
        if selected || interaction.get().hovered {
            theme.text.get()
        } else {
            theme.text_muted.get()
        }
    };
    let padding = if icon.is_some() {
        tokens::CONTROL_PADDING_X_SMALL
    } else {
        tokens::CONTROL_PADDING_X_MEDIUM
    };
    let detail = detail.filter(|_| icon.is_some()).map(|detail| {
        view! {
            <div test_id={format!("{id}-detail")}>
                <text small color=foreground>{detail}</text>
            </div>
        }
    });
    let (caption, hint) = if icon.is_some() {
        (None, Some(label))
    } else {
        (Some(label), None)
    };
    let root = view! {
        <div test_id=id h=24.0 flex items_center gap=4.0 px=padding rounded=tokens::RADIUS_INNER
             when:bg=(selected, theme.surface_selected) when:opacity=(disabled, tokens::DISABLED_OPACITY)
             track_interaction=interaction>
            {icon.map(|icon| Icon::new(icon, foreground))}
            {detail}
            {caption.map(|caption| view! { <text small color=foreground>{caption}</text> })}
        </div>
    };
    pressable(root, true, !disabled, on_click).hint(hint)
}

#[component]
pub fn disclosure(
    #[prop(into)] id: String,
    expanded: bool,
    #[prop(default, into)] on_toggle: Callback,
    children: impl View,
) -> Div {
    let root = view! {
        <div test_id=id flex items_center gap=6.0>
            {chevron(expanded)}
            <div>{children}</div>
        </div>
    };
    pressable(root, true, true, on_toggle)
}

pub(crate) fn chevron(expanded: bool) -> Icon {
    let icon = if expanded {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    Icon::new(icon, Theme::current().text_muted).size(tokens::ICON_SIZE_SMALL)
}

#[component]
pub fn muted_text(#[prop(into)] children: String) -> Text {
    text(children).small().muted()
}

#[component]
pub fn section_header(
    #[prop(into)] id: String,
    #[prop(into)] title: String,
    expanded: bool,
    #[prop(into)] detail: Option<String>,
    icon: Option<IconName>,
    badge: Option<AnyView>,
    action: Option<AnyView>,
    #[prop(default)] actions_always_visible: bool,
    #[prop(default, into)] on_toggle: Callback,
) -> Div {
    let interaction = signal(Interactions::default());
    let action = action.map(|action| {
        view! {
            <div opacity={move || if actions_always_visible || interaction.get().hovered { 1.0 } else { 0.0 }}>{action}</div>
        }
    });
    let toggle = format!("{id}-toggle");
    view! {
        <div test_id=id flex items_center justify_between px=8.0 h=tokens::GROUP_HEIGHT
             bg=Theme::current().chrome track_interaction=interaction>
            <disclosure id=toggle expanded on_toggle>
                <div flex items_center gap=6.0>
                    {icon.map(|icon| Icon::new(icon, Theme::current().text_muted))}
                    <text title primary>{title}</text>
                    {detail.map(|detail| view! { <muted_text>{detail}</muted_text> })}
                    {badge}
                </div>
            </disclosure>
            {action}
        </div>
    }
}

#[component]
pub fn list_row(
    #[prop(into)] id: String,
    #[prop(default)] depth: usize,
    #[prop(default)] state: SelectionState,
    #[prop(reactive)] selection: Option<Prop<SelectionState>>,
    #[prop(default, into)] on_click: Callback,
) -> Div {
    let theme = Theme::current();
    let selection = memo(move || {
        if state == SelectionState::Disabled {
            state
        } else {
            selection.as_ref().map_or(state, Prop::get)
        }
    });
    let interaction = signal(Interactions::default());
    let root = view! {
        <div flex items_center group={format!("{id}-row")} test_id=id min_h=tokens::ROW_HEIGHT
             pl={8.0 + indent(depth)} pr=8.0 gap=6.0 rounded=tokens::RADIUS_INNER
             track_interaction=interaction
             bg={move || match selection.get() {
                 SelectionState::None if interaction.get().hovered => theme.surface_hover.get(),
                 SelectionState::None | SelectionState::Disabled => Color::TRANSPARENT,
                 state => theme.selection_style(state).background.unwrap_or(Color::TRANSPARENT),
             }}
             text_color={move || theme.selection_style(selection.get()).foreground}/>
    };
    pressable(root, true, state != SelectionState::Disabled, on_click)
        .enabled(move || selection.get() != SelectionState::Disabled)
}

#[component]
pub fn action_bar() -> Div {
    view! {
        <div flex items_center justify_end gap=8.0 min_h=tokens::BAR_HEIGHT px=12.0 bg=Theme::current().chrome/>
    }
}

#[component]
pub fn backdrop(
    #[prop(into)] id: String,
    #[prop(default)] align_top: bool,
    anchor_below: Option<Signal<Rect>>,
    #[prop(default, into)] on_dismiss: Callback,
    children: impl View,
) -> impl View {
    let theme = Theme::current();
    overlay(move || {
        let inner = view! {
            <div flex col min_h=0.0 max_h=pct(100.0) max_w=pct(100.0)>{children}</div>
        };
        let root = view! {
            <div test_id=id flex col focus_scope autofocus tab_index=-1
                 on_key_down={move |event| {
                     if event.key != Key::Tab {
                         event.stop_propagation();
                     }
                 }}/>
        };
        let (root, inner) = if let Some(anchor) = anchor_below {
            (root, Placement::Below.place(inner, anchor))
        } else {
            let root = root.items_center().p(16.0).bg(theme.scrim);
            let root = if align_top {
                root.justify_start().pt(tokens::MODAL_TOP_OFFSET)
            } else {
                root.justify_center()
            };
            (root, inner)
        };
        dismissable(root, inner, move || on_dismiss.call())
    })
}

pub fn modal_width(size: ModalSize) -> Length {
    Length::design(match size {
        ModalSize::Compact => tokens::MODAL_WIDTH_COMPACT,
        ModalSize::Medium => tokens::MODAL_WIDTH,
        ModalSize::Wide => tokens::MODAL_WIDTH_WIDE,
    })
}

#[component]
pub fn modal(
    #[prop(into)] id: String,
    #[prop(into)] title: String,
    #[prop(into)] hint: Option<String>,
    #[prop(default)] size: ModalSize,
    height: Option<Length>,
    #[prop(default)] align_top: bool,
    #[prop(default, into)] on_dismiss: Callback,
    children: impl View,
) -> impl View {
    view! {
        <backdrop id={format!("{id}-backdrop")} align_top=align_top on_dismiss>
            <div test_id=id flex col gap=8.0 p=16.0 w={modal_width(size)} max_w=pct(100.0)
                 h?=height overflow_hidden material=Material::Floating>
                <text heading primary>{title}</text>
                {hint.map(|hint| view! { <muted_text>{hint}</muted_text> })}
                <div flex col min_h=0.0>{children}</div>
            </div>
        </backdrop>
    }
}

#[component]
pub fn empty_state(
    icon: IconName,
    #[prop(into)] title: String,
    #[prop(into)] detail: String,
) -> Div {
    view! {
        <div flex col items_center justify_center p=16.0 gap=6.0>
            {Icon::new(icon, Theme::current().text_muted)}
            <text title primary>{title}</text>
            <muted_text>{detail}</muted_text>
        </div>
    }
}

#[component]
pub fn notification(#[prop(into)] message: String, tone: NoticeTone) -> Div {
    let theme = Theme::current();
    let fg = move || theme.notice_style(tone).foreground;
    let icon = match tone {
        NoticeTone::Error | NoticeTone::Warning => IconName::TriangleAlert,
        NoticeTone::Positive => IconName::CircleCheck,
        _ => IconName::Info,
    };
    view! {
        <div flex items_center gap=8.0 p=8.0 rounded=tokens::RADIUS_CONTROL
             bg={move || theme.notice_style(tone).background.unwrap_or(Color::TRANSPARENT)}>
            {Icon::new(icon, fg)}
            <text body color=fg>{message}</text>
        </div>
    }
}

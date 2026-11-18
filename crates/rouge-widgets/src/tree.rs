use crate::util::{TestId, chrome};
use crate::{
    components::{ListRow, activate, chevron, list_row},
    icon::{Icon, IconName},
    tokens::{ICON_SIZE, ROW_HEIGHT, TREE_INDENT},
    typography::UiStyles,
};
use rouge_core::{
    AnyView, Apply, BuildCx, DelegateStyle, Div, Fill, FocusPolicy, IntoProp, Prop, Role,
    StyleDelta, Styled, View, component, div, text, view,
};
use rouge_reactive::Callback;
use rouge_theme::SelectionState;
use std::rc::Rc;

#[allow(clippy::cast_precision_loss)]
pub fn indent(depth: usize) -> f32 {
    depth as f32 * TREE_INDENT
}

pub struct TreeRow {
    label: String,
    depth: usize,
    expanded: Option<Prop<bool>>,
    icon: Option<IconName>,
    icon_color: Prop<Fill>,
    label_color: Prop<Fill>,
    leading: Option<AnyView>,
    trailing: Vec<AnyView>,
    selection: Prop<SelectionState>,
    on_click: Callback,
    on_toggle: Callback,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn tree_row(#[prop(into)] label: String) -> TreeRow {
    TreeRow {
        label,
        depth: 0,
        expanded: None,
        icon: None,
        icon_color: Prop::Static(Fill::Role(Role::TextMuted)),
        label_color: Prop::Static(Fill::Role(Role::Text)),
        leading: None,
        trailing: Vec::new(),
        selection: Prop::Static(SelectionState::None),
        on_click: Callback::default(),
        on_toggle: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl TreeRow {
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn expanded<T>(mut self, expanded: impl IntoProp<bool, T>) -> Self {
        self.expanded = Some(expanded.into_prop());
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn icon_color<T>(mut self, color: impl IntoProp<Fill, T>) -> Self {
        self.icon_color = color.into_prop();
        self
    }

    pub fn label_color<T>(mut self, color: impl IntoProp<Fill, T>) -> Self {
        self.label_color = color.into_prop();
        self
    }

    pub fn leading(mut self, leading: impl View) -> Self {
        self.leading = Some(leading.into_any());
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

    pub fn on_toggle(mut self, on_toggle: impl Into<Callback>) -> Self {
        self.on_toggle = on_toggle.into();
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.trailing.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.trailing
            .extend(children.into_iter().map(View::into_any));
        self
    }
}

chrome!(TreeRow);

impl View for TreeRow {
    fn build(self, cx: &mut BuildCx<'_>) {
        let TreeRow {
            label,
            depth,
            expanded,
            icon,
            icon_color,
            label_color,
            leading,
            trailing,
            selection,
            on_click,
            on_toggle,
            test_id,
            style,
        } = self;
        let selection = Rc::new(selection.memo());
        let slot = || {
            div()
                .size(ICON_SIZE)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
        };
        let caret = match expanded {
            Some(expanded) => {
                let toggle = test_id.part("toggle");
                let enabled = {
                    let selection = selection.clone();
                    move || selection.get() != SelectionState::Disabled
                };
                let caret = slot()
                    .when_some(toggle, Div::test_id)
                    .child(chevron(expanded))
                    .enabled(enabled);
                activate(caret, on_toggle, FocusPolicy::Tab)
            }
            None => slot(),
        };
        let icon = icon.map(|icon| Icon::new(icon, icon_color));
        let label_id = test_id.part("label");
        let row = list_row()
            .depth(depth)
            .selection(move || selection.get())
            .on_click(on_click)
            .h(ROW_HEIGHT)
            .when_some(test_id, ListRow::test_id)
            .child(caret)
            .child(leading)
            .child(icon)
            .child(view! {
                <div test_id?=label_id flex_1 min_w=0.0 overflow_hidden>
                    <text body no_wrap color=label_color>
                        {label}
                    </text>
                </div>
            })
            .children(trailing);
        style.apply(row).build(cx);
    }
}

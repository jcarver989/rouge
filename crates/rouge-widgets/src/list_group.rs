use crate::components::Grouped;
use crate::icon::{Icon, IconName};
use crate::tokens::{Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::chrome;
use rouge_core::{
    AnyView, Apply, BuildCx, DelegateStyle, Div, Role, StyleDelta, Styled, Theme, View, component,
    div, text, view,
};

pub struct ListGroup {
    title: Option<String>,
    rows: Vec<AnyView>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn list_group() -> ListGroup {
    ListGroup {
        title: None,
        rows: Vec::new(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl ListGroup {
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.rows.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.rows.extend(children.into_iter().map(View::into_any));
        self
    }
}

chrome!(ListGroup);

impl View for ListGroup {
    fn build(self, cx: &mut BuildCx<'_>) {
        let rows = self.rows.into_iter().enumerate().map(|(index, row)| {
            let rule = (index > 0).then(|| {
                view! {
                    <div absolute top=0.0 left=Space::X3 right=0.0 pass_through border_t=1.0
                         border_color=Role::BorderSubtle/>
                }
            });
            view! { <div relative>{row}{rule}</div> }
        });
        let title = self.title.map(|title| {
            view! { <div px=Space::X3 pb=Space::X2><text label muted no_wrap>{title}</text></div> }
        });
        let card = view! {
            <div relative flex col rounded=Radius::Control bg=Role::Elevated overflow_hidden>
                {..rows}
                <div absolute inset=0.0 pass_through rounded=Radius::Control border=1.0
                     border_color=Role::BorderSubtle/>
            </div>
        }
        .provide(Grouped);
        let root = view! {
            <div flex col>
                {title}
                {card}
            </div>
        };
        self.style
            .apply(root.when_some(self.test_id, Div::test_id))
            .build(cx);
    }
}

#[component]
pub fn icon_tile(icon: IconName) -> Div {
    let theme = Theme::current();
    view! {
        <div flex flex_none items_center justify_center size=28.0 rounded=Radius::Inner
             bg={move || theme.get().accent.with_alpha(TINT_ALPHA)}>
            {Icon::new(icon, Role::Accent)}
        </div>
    }
}

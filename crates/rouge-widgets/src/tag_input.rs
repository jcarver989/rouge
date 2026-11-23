use crate::icon::{Icon, IconName};
use crate::popover::POPPED;
use crate::tokens::{GROUP_HEIGHT, HAIRLINE, ICON_SIZE_SMALL, Pace, Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::{Name, TestId, chrome, entry, entry_frame, labelled};
use rouge_core::access;
use rouge_core::{
    Apply, BuildCx, Cursor, DelegateStyle, Editable, Key, Role, StyleDelta, Styled, Theme, View,
    component, div, each, focus_handle, text, view,
};
use rouge_reactive::{KeyedList, signal};

pub struct TagInput {
    tags: KeyedList<String, String>,
    placeholder: Option<String>,
    name: Name,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn tag_input(tags: KeyedList<String, String>) -> TagInput {
    TagInput {
        tags,
        placeholder: None,
        name: None,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl TagInput {
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }
}

chrome!(TagInput);
labelled!(TagInput);

impl View for TagInput {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (tags, id) = (self.tags, self.test_id);
        let (theme, focus) = (Theme::current(), focus_handle());
        let draft = signal(String::new());
        let add = move || {
            let tag = draft.get_untracked().trim().to_owned();
            if !tag.is_empty() && tags.with(&tag, |_| ()).is_none() {
                tags.push(tag);
            }
            draft.set(String::new());
        };
        let chip_id = id.clone();
        let chips = view! {
            <each items=tags.keys() key=Clone::clone let:tag enter=(POPPED, Pace::Enter)
                 exit=(POPPED, Pace::Exit)>
                {
                    let name = tag.get_untracked();
                    let part = |kind: &str| chip_id.part(format_args!("{kind}-{name}"));
                    let removed = name.clone();
                    let remove = view! {
                        <div test_id?={part("remove")} role=access::Role::Button
                             access_label={format!("Remove {name}")} flex_none cursor=Cursor::Pointer on_click={move |event| {
                            event.stop_propagation();
                            tags.remove(&removed);
                        }}>
                            {Icon::new(IconName::X, Role::Accent).size(ICON_SIZE_SMALL)}
                        </div>
                    };
                    view! {
                        <div test_id?={part("tag")} flex flex_none items_center gap=Space::X1 h=20.0 pl=Space::X2 pr=Space::X1 rounded=Radius::Inner
                             text_color=Role::Accent layout_transition=Pace::Layout
                             bg={move || theme.get().accent.with_alpha(TINT_ALPHA)}>
                            <text caption no_wrap weight=500.0>{name.clone()}</text>
                            {remove}
                        </div>
                    }
                }
            </each>
        };
        let fallback = self.placeholder.as_deref().or(Some("Add a tag"));
        let field = entry(draft, focus, self.name, fallback)
            .on_submit(move |_| add())
            .on_key_down(move |event| {
                if event.key == Key::Backspace && draft.with_untracked(String::is_empty) {
                    let last = tags.keys().with_untracked(|keys| keys.last().cloned());
                    if let Some(last) = last {
                        tags.remove(&last);
                        event.stop_propagation();
                    }
                }
            })
            .when_some(self.placeholder, |field, hint| {
                field.placeholder(hint, Role::TextMuted)
            })
            .when_some(id.part("input"), Editable::test_id);
        let root = view! {
            <div test_id?=id flex flex_wrap items_center gap=Space::X1 px=Space::X2 py={Space::X1.px() - HAIRLINE} min_h=GROUP_HEIGHT>
                {chips}
                <div flex_1 min_w=80.0 layout_transition=Pace::Layout>{field}</div>
            </div>
        };
        self.style.apply(entry_frame(root, focus)).build(cx);
    }
}

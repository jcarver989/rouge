use crate::components::{ControlSize, button};
use crate::effects::BusyExt;
use crate::input::text_input;
use crate::markdown::markdown;
use crate::material::{Material, MaterialExt};
use crate::tokens::{ROW_HEIGHT, Radius, Space};
use crate::typography::UiStyles;
use crate::util::{TestId, chrome};
use rouge_core::{
    BuildCx, Color, DelegateStyle, IntoProp, Prop, RowKey, StyleDelta, Styled, View, component,
    div, pct, text, view, virtual_list,
};
use rouge_markdown::MarkdownStream;
use rouge_reactive::{Callback, KeyedList, ReadSignal, Signal, signal};
use rouge_theme::ButtonVariant;
use std::hash::Hash;
use std::rc::Rc;

#[derive(Clone)]
pub enum Message {
    User(String),
    Assistant(ReadSignal<MarkdownStream>),
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Message::User(text.into())
    }

    pub fn assistant(stream: Signal<MarkdownStream>) -> Self {
        Message::Assistant(stream.read_only())
    }
}

pub struct Transcript<T: 'static, U: 'static> {
    messages: KeyedList<T, U>,
    message: Rc<dyn Fn(&U) -> Message>,
    following: Option<Signal<bool>>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn transcript<T: Eq + Hash + Clone + 'static, U: 'static>(
    messages: KeyedList<T, U>,
    message: impl Fn(&U) -> Message + 'static,
) -> Transcript<T, U> {
    Transcript {
        messages,
        message: Rc::new(message),
        following: None,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl<T, U> Transcript<T, U> {
    pub fn following(mut self, following: Signal<bool>) -> Self {
        self.following = Some(following);
        self
    }
}

chrome!(impl[T, U] Transcript<T, U>);

impl<T: Eq + Hash + Clone + 'static, U: 'static> View for Transcript<T, U> {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (messages, read, id) = (self.messages, self.message, self.test_id);
        let following = self.following.unwrap_or_else(|| signal(true));
        let keys = messages.keys();
        let row_id = id.clone();
        let list = view! {
            <virtual_list count={move || keys.with(Vec::len)} row_height=ROW_HEIGHT let:index
                 row_identity={move |index| RowKey::new(keys.with(|keys| keys.get(index).cloned()), 0)}
                 measure_rows={|| 0} follow_end=following flex_1 min_h=0.0>
                {
                    let message = keys
                        .with_untracked(|keys| keys.get(index).cloned())
                        .and_then(|key| messages.with(&key, |message| read(message)));
                    let bubble = match message {
                        Some(Message::User(said)) => view! {
                            <div flex justify_end>
                                <div max_w=pct(80.0) px=Space::X3 py=Space::X2 rounded=Radius::Floating material=Material::Pigment>
                                    <text body>{said}</text>
                                </div>
                            </div>
                        },
                        Some(Message::Assistant(stream)) => view! {
                            <div>
                                <markdown source=stream compact/>
                            </div>
                        },
                        None => view! { <div/> },
                    };
                    view! {
                        <div py=Space::X1 test_id?=row_id.part(format_args!("message-{index}"))>{bubble}</div>
                    }
                }
            </virtual_list>
        };
        let root = view! { <div flex col min_h=0.0 test_id?=id>{list}</div> };
        self.style.apply(root).build(cx);
    }
}

pub struct Composer {
    draft: Signal<String>,
    placeholder: Option<String>,
    busy: Prop<bool>,
    on_send: Callback<(String,)>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn composer(draft: Signal<String>) -> Composer {
    Composer {
        draft,
        placeholder: None,
        busy: Prop::Static(false),
        on_send: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Composer {
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn busy<T>(mut self, busy: impl IntoProp<bool, T>) -> Self {
        self.busy = busy.into_prop();
        self
    }

    pub fn on_send(mut self, on_send: impl Into<Callback<(String,)>>) -> Self {
        self.on_send = on_send.into();
        self
    }
}

chrome!(Composer);

impl View for Composer {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (draft, on_send, id) = (self.draft, self.on_send, self.test_id);
        let busy = Rc::new(self.busy.memo());
        let part = |name: &str| id.part(format_args!("{name}"));
        let working = {
            let busy = busy.clone();
            move || busy.get()
        };
        let send = {
            let working = working.clone();
            move || {
                let said = draft.get_untracked().trim().to_owned();
                if !said.is_empty() && !working() {
                    draft.set(String::new());
                    on_send.call(said);
                }
            }
        };
        let field = view! {
            <text_input value=draft multiline min_rows=1 max_rows=MAX_ROWS submit_on_enter
                 on_submit={{
                     let send = send.clone();
                     move |_: String| send()
                 }}
                 flex_1 min_w=0.0 bg=Color::TRANSPARENT border_color=Color::TRANSPARENT
                 placeholder?=self.placeholder test_id?=part("input")/>
        };
        let idle = {
            let working = working.clone();
            move || working() || draft.with(|draft| draft.trim().is_empty())
        };
        let action = view! {
            <button label="Send" variant=ButtonVariant::Primary control_size=ControlSize::Small
                 disabled=idle on_click=send test_id?=part("send")/>
        };
        let root = view! {
            <div flex items_end gap=Space::X2 p=Space::X2 material=Material::Glass test_id?=id>
                {field}
                {action}
            </div>
        }
        .busy(working, Radius::Floating);
        self.style.apply(root).build(cx);
    }
}

const MAX_ROWS: usize = 6;

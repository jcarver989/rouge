use crate::input::text_input;
use crate::markdown::markdown;
use crate::material::{Material, MaterialExt};
use crate::progress::spinner;
use crate::tokens::{DISABLED_OPACITY, PRESS_SCALE, Pace, ROW_HEIGHT, Radius, Space};
use crate::typography::UiStyles;
use crate::util::{TestId, chrome, focus_ring};
use rouge_core::{
    Apply, BuildCx, Color, Cursor, DelegateStyle, Div, FocusPolicy, IntoProp, LayoutTransition,
    Pose, Prop, Role, StyleDelta, Styled, Theme, View, component, div, pct, show, text, view,
    virtual_list,
};
use rouge_markdown::MarkdownStream;
use rouge_reactive::{Callback, KeyedList, ReadSignal, Signal, memo, signal};
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
        let keys = messages.order();
        let row_id = id.clone();
        let list = view! {
            <virtual_list count=messages row_height=ROW_HEIGHT let:index
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
                        <div py=Space::X1>{bubble}</div>
                    }.when_some(row_id.row("message"), Div::test_id)
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
    on_stop: Callback,
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
        on_stop: Callback::default(),
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

    pub fn on_stop(mut self, on_stop: impl Into<Callback>) -> Self {
        self.on_stop = on_stop.into();
        self
    }
}

chrome!(Composer);

#[derive(Clone, Copy, Eq, PartialEq)]
enum ComposerAction {
    Send,
    Busy,
    Stop,
}

impl View for Composer {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (draft, on_send, on_stop, id) = (self.draft, self.on_send, self.on_stop, self.test_id);
        let can_stop = on_stop.is_set();
        let state = memo(move || match (self.busy.get(), can_stop) {
            (false, _) => ComposerAction::Send,
            (true, false) => ComposerAction::Busy,
            (true, true) => ComposerAction::Stop,
        });
        let working = move || state.get() != ComposerAction::Send;
        let interactive = move || match state.get() {
            ComposerAction::Send => !draft.with(|draft| draft.trim().is_empty()),
            ComposerAction::Busy => false,
            ComposerAction::Stop => true,
        };
        let theme = Theme::current();
        let part = |name: &str| id.part(format_args!("{name}"));
        let send = move || {
            let said = draft.get_untracked().trim().to_owned();
            if !working() && !said.is_empty() {
                draft.set(String::new());
                on_send.call(said);
            }
        };
        let field = view! {
            <text_input value=draft multiline min_rows=1 max_rows=MAX_ROWS submit_on_enter
                 on_submit={move |_: String| send()}
                 flex_1 min_w=0.0 bg=Color::TRANSPARENT border_color=Color::TRANSPARENT
                 placeholder?=self.placeholder test_id?=part("input")/>
        };
        let pressed = signal(false);
        let activate = move || match state.get() {
            ComposerAction::Send => send(),
            ComposerAction::Stop => on_stop.call(),
            ComposerAction::Busy => (),
        };
        let spinner_id = part("spinner");
        let stop_id = part("stop");
        let indicator = view! {
            <show when=working enter=(MORPH_HIDDEN, Pace::Enter) exit=(MORPH_HIDDEN, Pace::Exit)>
                <div test_id?={spinner_id.clone()} absolute inset=0.0 flex items_center justify_center pass_through>
                    {spinner().size(STOP_DIAMETER)}
                    {can_stop.then(|| view! {
                        <div test_id?={stop_id.clone()} absolute size=STOP_GLYPH rounded=Radius::Inner bg=Role::Text/>
                    })}
                </div>
            </show>
        };
        let label = view! {
            <div flex items_center justify_center
                 opacity={move || if working() { 0.0 } else { 1.0 }}
                 scale={move || if working() { MORPH_SCALE } else { 1.0 }}
                 transition=Pace::Move>
                <text label no_wrap text_color=Role::AccentForeground>"Send"</text>
            </div>
        };
        let action = focus_ring(view! {
            <div relative flex items_center justify_center flex_none overflow_hidden
                 h=STOP_DIAMETER w={move || if working() { STOP_DIAMETER } else { SEND_WIDTH }}
                 rounded={move || if working() { Radius::Full } else { Radius::Control }}
                 border=1.0 border_color=Color::TRANSPARENT
                 bg={move || if working() { Role::SurfaceSelected } else { Role::Accent }}
                 opacity={move || if working() || interactive() { 1.0 } else { DISABLED_OPACITY }}
                 on_press={move |down| pressed.set(down)}
                 scale={move || if pressed.get() { PRESS_SCALE } else { 1.0 }}
                 transition=Pace::Move
                 layout_transition={LayoutTransition::new(Pace::Move).size()}
                 test_id?=part("send")>
                {label}
                {indicator}
            </div>
        })
        .enabled(interactive)
        .on_activate(activate)
        .cursor(move || {
            if interactive() {
                Cursor::Pointer
            } else {
                Cursor::Default
            }
        })
        .focus_policy(FocusPolicy::Tab);
        let root = view! {
            <div flex items_end gap=Space::X2 p=Space::X2 material=Material::Glass
                 relative test_id?=id>
                {field}
                {action}
                <div absolute inset=0.0 pass_through rounded=Radius::Floating
                     border=1.0 border_color={move || {
                         if working() {
                             theme.get().accent.with_alpha(BUSY_BORDER_ALPHA)
                         } else {
                             Color::TRANSPARENT
                         }
                     }}/>
            </div>
        };
        self.style.apply(root).build(cx);
    }
}

const MAX_ROWS: usize = 6;
const SEND_WIDTH: f32 = 56.0;
const STOP_DIAMETER: f32 = 28.0;
const STOP_GLYPH: f32 = 8.0;
const MORPH_SCALE: f32 = 0.7;
const BUSY_BORDER_ALPHA: f32 = 0.34;
const MORPH_HIDDEN: Pose = Pose::new().opacity(0.0).scale(MORPH_SCALE);

use crate::components::{empty_state, icon_button};
use crate::icon::{Icon, IconName};
use crate::progress::spinner;
use crate::style::UiStyle;
use crate::tokens::{BAR_HEIGHT, HAIRLINE, ICON_SIZE_SMALL, Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::{TestId, by_touch, chrome};
use rouge_core::{
    AnyView, BoxShadow, BuildCx, Corners, Cursor, DelegateStyle, Div, Fill, FocusHandle, Key, Role,
    Size, StyleDelta, Styled, Theme, View, component, div, focus_handle, show, text, view,
};
use rouge_reactive::{Callback, signal};
use rouge_webview::{WebPage, web_view};

pub struct WebCard {
    page: WebPage,
    name: String,
    icon: IconName,
    tag: Option<String>,
    page_size: Size,
    fill: bool,
    focus: Option<FocusHandle>,
    actions: Vec<AnyView>,
    on_open: Callback,
    on_expand: Option<Callback>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn web_card(page: WebPage, #[prop(into)] name: String) -> WebCard {
    WebCard {
        page,
        name,
        icon: IconName::Globe,
        tag: None,
        page_size: PAGE,
        fill: false,
        focus: None,
        actions: Vec::new(),
        on_open: Callback::default(),
        on_expand: None,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl WebCard {
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = icon;
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    pub fn page_size(mut self, width: f32, height: f32) -> Self {
        self.page_size = Size::new(width, height);
        self
    }

    pub fn fill_height(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn track_focus(mut self, focus: FocusHandle) -> Self {
        self.focus = Some(focus);
        self
    }

    pub fn action(mut self, action: impl View) -> Self {
        self.actions.push(action.into_any());
        self
    }

    pub fn on_open(mut self, on_open: impl Into<Callback>) -> Self {
        self.on_open = on_open.into();
        self
    }

    pub fn on_expand(mut self, on_expand: impl Into<Callback>) -> Self {
        self.on_expand = Some(on_expand.into());
        self
    }
}

chrome!(WebCard);

impl View for WebCard {
    fn build(self, cx: &mut BuildCx<'_>) {
        let page = self.page;
        let mode = match self.on_expand {
            Some(on_expand) => Mode::Expands(on_expand),
            None => Mode::Interacts(self.focus.unwrap_or_else(focus_handle)),
        };
        let unavailable = page.error().is_some();
        let hint = by_touch("Tap", "Click");
        let state = page.state();
        let status = move || {
            let status = if unavailable {
                Status::Unavailable
            } else if let Some(reason) = state.with(|state| state.failure.clone()) {
                Status::Failed(reason)
            } else if state.with(|state| state.loading) {
                Status::Loading
            } else if mode.expands().is_some() {
                Status::Opens
            } else if mode.awake() {
                Status::Awake
            } else {
                Status::Asleep
            };
            status.label(hint)
        };
        let expand = mode.expands().map(|on_expand| {
            view! {
                <icon_button icon=IconName::Maximize label="Expand" on_click=on_expand disabled=unavailable
                     test_id?=self.test_id.part("expand")/>
            }
        });
        let open = self.on_open.is_set().then(|| {
            view! {
                <icon_button icon=IconName::ExternalLink label="Open in the browser" on_click=self.on_open
                     test_id?=self.test_id.part("open")/>
            }
        });
        let header = view! {
            <div flex flex_none items_center gap=Space::X2 min_h=BAR_HEIGHT border_b=HAIRLINE
                 border_color=Role::BorderSubtle pl=Space::X3 pr=Space::X2>
                {Icon::new(self.icon, Role::TextSecondary).size(ICON_SIZE_SMALL)}
                <text code small primary truncate>{self.name.clone()}</text>
                {self.tag.map(tag)}
                <div flex_1 min_w=0.0>
                    <text small muted truncate test_id?=self.test_id.part("status")>{status}</text>
                </div>
                <show when={move || state.with(|state| state.loading)}>
                    <spinner size=ICON_SIZE_SMALL flex_none/>
                </show>
                {..self.actions}
                {expand}
                <icon_button icon=IconName::RotateCw label="Reload" on_click={move || page.reload()}
                     disabled=unavailable test_id?=self.test_id.part("reload")/>
                {open}
            </div>
        };

        let width = signal(Size::ZERO);
        let inner = UiStyle::current().radii.px(Radius::Floating) - HAIRLINE;
        let (page_size, fill) = (self.page_size, self.fill);
        let fitted = move || {
            let fitted = width.get().width / page_size.width;
            if fitted > 0.0 { fitted } else { 1.0 }
        };
        let corners = Corners {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: inner,
            bottom_left: inner,
        };
        let expands = mode.expands().filter(|_| !unavailable);
        let viewport = view! {
            <div relative w_full bg=Role::Canvas track_size=width
                 when:flex_1=fill when:min_h=(fill, 0.0)
                 when:flex_none={!fill} when:aspect_ratio=(!fill, page_size.width / page_size.height)
                 cursor={move || (!mode.awake() && !unavailable).then_some(Cursor::Pointer)}
                 on_key_down={move |event| {
                     if event.key == Key::Escape
                         && let Some(focus) = mode.focus().filter(FocusHandle::is_focused)
                     {
                         event.stop_propagation();
                         focus.blur();
                     }
                 }}
                 when:on_click=(expands.is_some(), move |_| expands.unwrap_or_default().call())
                 test_id?=self.test_id.part("viewport")>
                <web_view page absolute inset=0.0 rounded=corners
                     interactive={move || mode.awake()} page_zoom=fitted
                     track_focus?=mode.focus() access_label={format!("{} preview", self.name)}
                     fallback={|error| view! {
                         <div size_full flex items_center justify_center>
                             <empty_state icon=IconName::Unplug title="No web view here" detail={error.to_string()}/>
                         </div>
                     }}
                     test_id?=self.test_id.part("page")/>
            </div>
        };

        let theme = Theme::current();
        let ring = move || {
            let alpha = if mode.awake() { TINT_ALPHA / 2.0 } else { 0.0 };
            let color = theme.resolve(Fill::Role(Role::Text)).with_alpha(alpha);
            BoxShadow::new(0.0, 0.0, 0.0, color).spread(3.0)
        };
        let border = move || {
            if mode.awake() {
                Role::TextMuted
            } else {
                Role::BorderSubtle
            }
        };
        let root = view! {
            <div flex col min_w=0.0 rounded=Radius::Floating border=HAIRLINE border_color=border
                 bg=Role::Surface shadow=ring overflow_hidden test_id?=self.test_id>
                {header}
                {viewport}
            </div>
        };
        self.style.apply(root).build(cx);
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Interacts(FocusHandle),
    Expands(Callback),
}

impl Mode {
    fn focus(self) -> Option<FocusHandle> {
        match self {
            Mode::Interacts(focus) => Some(focus),
            Mode::Expands(_) => None,
        }
    }

    fn expands(self) -> Option<Callback> {
        match self {
            Mode::Expands(on_expand) => Some(on_expand),
            Mode::Interacts(_) => None,
        }
    }

    fn awake(self) -> bool {
        self.focus().is_some_and(|focus| focus.is_focused())
    }
}

enum Status {
    Unavailable,
    Failed(String),
    Loading,
    Opens,
    Awake,
    Asleep,
}

impl Status {
    fn label(&self, press: &str) -> String {
        match self {
            Status::Unavailable => "Unavailable here".to_owned(),
            Status::Failed(reason) => format!("Couldn't load: {reason}"),
            Status::Loading => "Loading".to_owned(),
            Status::Opens => format!("{press} to open"),
            Status::Awake => format!("{press} outside to return"),
            Status::Asleep => format!("{press} to interact"),
        }
    }
}

fn tag(tag: String) -> Div {
    let theme = Theme::current();
    let tint = move || {
        theme
            .resolve(Fill::Role(Role::Text))
            .with_alpha(TINT_ALPHA / 2.0)
    };
    view! {
        <div flex_none px=Space::X1 rounded=Radius::Inner bg=tint>
            <text caption secondary no_wrap>{tag}</text>
        </div>
    }
}

const PAGE: Size = Size::new(960.0, 600.0);

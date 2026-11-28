use crate::components::{empty_state, icon_button};
use crate::icon::{Icon, IconName};
use crate::progress::spinner;
use crate::style::UiStyle;
use crate::tokens::{BAR_HEIGHT, HAIRLINE, ICON_SIZE_SMALL, Pace, Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::{TestId, by_touch, chrome};
use rouge_core::{
    AnyView, BoxShadow, BuildCx, Corners, Cursor, DelegateStyle, Div, Fill, FocusHandle, Key, Rect,
    Role, Size, StyleDelta, Styled, Theme, Timing, View, access, component, div, focus_handle,
    motion, overlay, safe_area, show, text, view, window_size,
};
use rouge_reactive::{Callback, Signal, memo, signal, spawn};
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
        let on_expand = self.on_expand;
        let focus = on_expand
            .is_none()
            .then(|| self.focus.unwrap_or_else(focus_handle));
        let awake = move || focus.is_some_and(|focus| focus.is_focused());
        let unavailable = page.error().is_some();
        let hint = by_touch("Tap", "Click");
        let state = page.state();
        let status = move || {
            state.with(|state| match &state.failure {
                _ if unavailable => "Unavailable here".to_owned(),
                Some(reason) => format!("Couldn't load: {reason}"),
                None if state.loading => "Loading".to_owned(),
                None if on_expand.is_some() => format!("{hint} to open"),
                None if awake() => format!("{hint} outside to return"),
                None => format!("{hint} to interact"),
            })
        };
        let home = signal(Rect::ZERO);
        let preview_label = format!("{} preview", self.name);
        let header = Header {
            page,
            name: self.name,
            icon: self.icon,
            tag: self.tag,
            open: self.on_open,
        };
        let (expand, expand_label, screen) = if let Some(callback) = on_expand {
            (callback, "Expand", None)
        } else {
            let (enter, screen) = full_screen(header.clone(), home, self.test_id.clone());
            (enter, "Full screen", Some(screen))
        };
        let status = view! { <text small muted truncate test_id?=self.test_id.part("status")>{status}</text> };
        let mut actions = self.actions;
        actions.push(view! {
            <icon_button icon=IconName::Maximize label=expand_label on_click=expand disabled=unavailable
                 test_id?=self.test_id.part("expand")/>
        }.into_any());
        let header = header.bar(Some(status.into_any()), actions, &self.test_id);
        let inner = UiStyle::current().radii.px(Radius::Floating) - HAIRLINE;
        let (page_size, fill) = (self.page_size, self.fill);
        let fitted = move || {
            let fitted = home.get().size.width / page_size.width;
            if fitted > 0.0 { fitted } else { 1.0 }
        };
        let corners = Corners {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: inner,
            bottom_left: inner,
        };
        let expands = on_expand.filter(|_| !unavailable);
        let viewport = view! {
            <div relative w_full bg=Role::Canvas track_bounds=home
                 when:flex_1=fill when:min_h=(fill, 0.0)
                 when:flex_none={!fill} when:aspect_ratio=(!fill, page_size.width / page_size.height)
                 cursor={move || (!awake() && !unavailable).then_some(Cursor::Pointer)}
                 on_key_down={move |event| {
                     if event.key == Key::Escape
                         && let Some(focus) = focus.filter(FocusHandle::is_focused)
                     {
                         event.stop_propagation();
                         focus.blur();
                     }
                 }}
                 when:on_click=(expands.is_some(), move |_| expands.unwrap_or_default().call())
                 test_id?=self.test_id.part("viewport")>
                <web_view page absolute inset=0.0 rounded=corners
                     interactive=awake page_zoom=fitted
                     track_focus?=focus access_label=preview_label
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
            let alpha = if awake() { TINT_ALPHA / 2.0 } else { 0.0 };
            let color = theme.resolve(Fill::Role(Role::Text)).with_alpha(alpha);
            BoxShadow::new(0.0, 0.0, 0.0, color).spread(3.0)
        };
        let border = move || {
            if awake() {
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
                {screen}
            </div>
        };
        self.style.apply(root).build(cx);
    }
}

#[derive(Clone)]
struct Header {
    page: WebPage,
    name: String,
    icon: IconName,
    tag: Option<String>,
    open: Callback,
}

impl Header {
    fn bar(&self, status: Option<AnyView>, actions: Vec<AnyView>, id: &impl TestId) -> Div {
        let (page, open) = (self.page, self.open);
        let state = page.state();
        view! {
            <div flex flex_none items_center gap=Space::X2 min_h=BAR_HEIGHT border_b=HAIRLINE
                 border_color=Role::BorderSubtle pl=Space::X3 pr=Space::X2>
                {title(self.icon, self.name.clone(), self.tag.clone())}
                <div flex_1 min_w=0.0>{status}</div>
                <show when={move || state.with(|state| state.loading)}>
                    <spinner size=ICON_SIZE_SMALL flex_none/>
                </show>
                {..actions}
                <icon_button icon=IconName::RotateCw label="Reload" on_click={move || page.reload()}
                     disabled={page.error().is_some()} test_id?=id.part("reload")/>
                {open.is_set().then(|| view! {
                    <icon_button icon=IconName::ExternalLink label="Open in the browser" on_click=open
                         test_id?=id.part("open")/>
                })}
            </div>
        }
    }
}

fn full_screen(header: Header, home: Signal<Rect>, id: Option<String>) -> (Callback, impl View) {
    let page = header.page;
    let (full_screen, grown) = (signal(false), motion(0.0));
    let (grow, shrink): (Timing, Timing) = (Pace::Layout.into(), Pace::Exit.into());
    let enter = move || {
        full_screen.set(true);
        grown.animate_to(1.0, grow);
    };
    let leave = move || {
        let landed = grown.animate_to(0.0, shrink);
        spawn(async move {
            if landed.await {
                full_screen.set(false);
            }
        });
    };
    let (insets, window) = (safe_area(), window_size());
    let away = signal(Rect::ZERO);
    let full_size = move || {
        let (measured, window) = (away.get().size, window.get());
        if measured.width > 0.0 {
            measured
        } else {
            Size::new(window.width, (window.height - BAR_HEIGHT).max(1.0))
        }
    };
    let frame = memo(move || {
        let (home, away) = (home.get(), away.get());
        if away.is_empty() {
            home
        } else {
            between(home, away, grown.value())
        }
    });
    let screen = move || {
        view! {
            <div absolute inset=0.0 flex col focus_scope autofocus tab_index=-1
                 pt={move || insets.get().top} pr={move || insets.get().right}
                 pb={move || insets.get().bottom} pl={move || insets.get().left}
                 role=access::Role::Dialog access_label={header.name.clone()}
                 on_key_down={move |event| {
                     if event.key == Key::Escape {
                         event.stop_propagation();
                         leave();
                     }
                 }}
                 test_id?=id.part("screen")>
                <div absolute inset=0.0 bg=Role::Canvas opacity=grown/>
                {header.bar(None, Vec::new(), &None).bg(Role::Surface)
                    .opacity(grown.map(HEADER_ENTERS, [0.0, 1.0]))
                    .translate_y(grown.map(HEADER_ENTERS, [-HEADER_DROP, 0.0]))
                    .child(view! {
                        <icon_button icon=IconName::Minimize label="Exit full screen" on_click=leave
                             test_id?=id.part("collapse")/>
                    })}
                <div flex_1 min_h=0.0 track_bounds=away/>
                <div absolute overflow_hidden
                     left={move || frame.get().origin.x} top={move || frame.get().origin.y}
                     w={move || frame.get().size.width} h={move || frame.get().size.height}>
                    <web_view page exclusive size_full page_size=full_size
                         access_label={header.name.clone()} test_id?=id.part("screen-page")/>
                </div>
            </div>
        }
    };
    (
        enter.into(),
        view! { <overlay><show when={move || full_screen.get()} children=screen/></overlay> },
    )
}

fn title(icon: IconName, name: String, label: Option<String>) -> Div {
    view! {
        <div flex items_center gap=Space::X2 min_w=0.0>
            {Icon::new(icon, Role::TextSecondary).size(ICON_SIZE_SMALL)}
            <text code small primary truncate>{name}</text>
            {label.map(tag)}
        </div>
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

fn between(from: Rect, to: Rect, progress: f32) -> Rect {
    let mix = |from: f32, to: f32| (from + (to - from) * progress).round();
    Rect::new(
        mix(from.origin.x, to.origin.x),
        mix(from.origin.y, to.origin.y),
        mix(from.size.width, to.size.width),
        mix(from.size.height, to.size.height),
    )
}

const PAGE: Size = Size::new(960.0, 600.0);
const HEADER_ENTERS: [f32; 2] = [0.3, 0.9];
const HEADER_DROP: f32 = 6.0;

use crate::components::{empty_state, icon_button};
use crate::icon::{Icon, IconName};
use crate::progress::spinner;
use crate::style::UiStyle;
use crate::tokens::{
    BAR_HEIGHT, HAIRLINE, ICON_SIZE, ICON_SIZE_SMALL, MotionScale, Radius, Space, TINT_ALPHA,
};
use crate::typography::UiStyles;
use crate::util::{TestId, by_touch, chrome};
use rouge_core::{
    AnyView, BackdropBlur, BoxShadow, BuildCx, Color, Corners, Cursor, DelegateStyle, Div, Ease,
    Edges, Fill, FocusHandle, Key, Motion, PaintRegion, Rect, Role, Size, StyleDelta, Styled,
    Theme, Tween, View, access, component, div, focus_handle, motion, overlay, safe_area, show,
    text, view,
};
use rouge_reactive::{Callback, Signal, memo, signal, spawn};
use rouge_webview::{WebPage, web_view};
use std::f32::consts::PI;

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
        let (page_size, fill) = (self.page_size, self.fill);
        let home = signal(Rect::ZERO);
        let card = signal(PaintRegion::default());
        let fitted = move || {
            let fitted = home.get().size.width / page_size.width;
            if fitted > 0.0 { fitted } else { 1.0 }
        };
        let inner = UiStyle::current().radii.px(Radius::Floating) - HAIRLINE;
        let preview_label = format!("{} preview", self.name);
        let header = Header {
            page,
            name: self.name,
            icon: self.icon,
            tag: self.tag,
            open: self.on_open,
        };
        let morph = on_expand.is_none().then(Morph::new);
        let (expand, expand_label) = match (on_expand, morph) {
            (Some(callback), _) => (callback, "Expand"),
            (None, morph) => (
                Callback::from(move || {
                    if let Some(morph) = morph {
                        morph.expand();
                    }
                }),
                "Full screen",
            ),
        };
        let screen = morph
            .map(|morph| full_screen(header.clone(), card, status, morph, self.test_id.clone()));
        let status = view! { <text small muted truncate test_id?=self.test_id.part("status")>{status}</text> };
        let mut actions = self.actions;
        actions.push(view! {
            <icon_button icon=IconName::Maximize label=expand_label on_click=expand disabled=unavailable
                 test_id?=self.test_id.part("expand")/>
        }.into_any());
        let header = header.bar(Some(status.into_any()), actions, &self.test_id);
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
                     page_opacity={move || morph.map_or(1.0, |morph| morph.fade.value())}
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
        let lifted = move || morph.is_some_and(|morph| morph.shown.get());
        let root = view! {
            <div flex col min_w=0.0 rounded=Radius::Floating border=HAIRLINE border_color=border
                 bg=Role::Surface shadow=ring overflow_hidden track_paint_region=card
                 opacity={move || if lifted() { 0.0 } else { 1.0 }} test_id?=self.test_id>
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

#[derive(Clone, Copy)]
struct Morph {
    shown: Signal<bool>,
    grown: Motion,
    fade: Motion,
    turn: Signal<u64>,
    still: bool,
}

impl Morph {
    fn new() -> Self {
        Morph {
            shown: signal(false),
            grown: motion(0.0),
            fade: motion(1.0),
            turn: signal(0),
            still: MotionScale::current() == MotionScale::None,
        }
    }

    fn landed(self) -> bool {
        self.shown.get() && self.grown.value() >= 1.0
    }

    fn expand(self) {
        let Some(current) = self.begin(true) else {
            return;
        };
        spawn(async move {
            if !self.shown.get_untracked() {
                if !self.fade.animate_to(0.0, FADE_OUT).await || !current() {
                    return;
                }
                self.shown.set(true);
            }
            if self.grown.animate_to(1.0, GROW).await && current() {
                self.fade.animate_to(1.0, FADE_IN);
            }
        });
    }

    fn collapse(self) {
        let Some(current) = self.begin(false) else {
            return;
        };
        spawn(async move {
            if self.shown.get_untracked() {
                if !self.fade.animate_to(0.0, FADE_OUT).await || !current() {
                    return;
                }
                if !self.grown.animate_to(0.0, SHRINK).await || !current() {
                    return;
                }
                self.shown.set(false);
            }
            self.fade.animate_to(1.0, FADE_IN);
        });
    }

    fn begin(self, expand: bool) -> Option<impl Fn() -> bool + 'static> {
        let turn = self.turn.get_untracked() + 1;
        self.turn.set(turn);
        if self.still {
            self.shown.set(expand);
            self.grown.set(f32::from(expand));
            self.fade.set(1.0);
            return None;
        }
        Some(move || self.turn.get_untracked() == turn)
    }
}

fn full_screen(
    header: Header,
    card: Signal<PaintRegion>,
    status: impl Fn() -> String + Clone + 'static,
    morph: Morph,
    id: Option<String>,
) -> impl View {
    let (page, grown) = (header.page, morph.grown);
    let leave = move || morph.collapse();
    let (insets, theme) = (safe_area(), Theme::current());
    let radius = UiStyle::current().radii.px(Radius::Floating);
    let (window, area) = (signal(Rect::ZERO), signal(Rect::ZERO));
    let progress = move || grown.value().clamp(0.0, 1.0);
    let toward = move |from| {
        let to = window.get();
        if to.is_empty() {
            from
        } else {
            between(from, to, grown.value())
        }
    };
    let frame = memo(move || card.with(|region| toward(region.frame)));
    let clip = memo(move || card.with(|region| toward(region.bounds)));
    let corners = move || Corners::all(radius * (1.0 - progress()));
    let border = move || {
        let color = theme.resolve(Fill::Role(Role::BorderSubtle));
        color.with_alpha(color.a * (1.0 - progress()))
    };
    let lift = move || {
        let lift = (PI * progress()).sin();
        BoxShadow::new(
            0.0,
            LIFT_Y * lift,
            LIFT_BLUR * lift,
            Color::BLACK.with_alpha(LIFT_ALPHA * lift),
        )
    };
    let inset = move |side: fn(Edges) -> f32| side(insets.get()) * progress();
    let reveal = move |length: f32| if morph.landed() { length } else { 0.0 };
    let screen = move || {
        let status = view! {
            <div min_w=0.0 opacity=grown.map(LEAVES, [1.0, 0.0])>
                <text small muted truncate>{status.clone()}</text>
            </div>
        };
        let swap = view! {
            <div relative flex_none>
                <div opacity=grown.map(ENTERS, [0.0, 1.0])>
                    <icon_button icon=IconName::Minimize label="Exit full screen" on_click=leave
                         test_id?=id.part("collapse")/>
                </div>
                <div absolute inset=0.0 flex items_center justify_center pass_through access_hidden=true
                     opacity=grown.map(LEAVES, [1.0, 0.0])>
                    {Icon::new(IconName::Maximize, Role::TextMuted).size(ICON_SIZE)}
                </div>
            </div>
        };
        let bar = header.bar(Some(status.into_any()), vec![swap.into_any()], &None);
        view! {
                <div absolute inset=0.0 focus_scope autofocus tab_index=-1 track_bounds=window
                     role=access::Role::Dialog access_label={header.name.clone()}
                     on_key_down={move |event| {
                         if event.key == Key::Escape {
                             event.stop_propagation();
                             leave();
                         }
                     }}
                     test_id?=id.part("screen")>
                    <div absolute inset=0.0 pass_through bg=Role::Scrim backdrop=BackdropBlur::new(SCRIM_BLUR)
                         opacity=grown.map(SCRIM_RISES, [0.0, 1.0])/>
                    <div absolute rounded=corners bg=Role::Surface shadow=lift
                         left={move || clip.get().origin.x} top={move || clip.get().origin.y}
                         w={move || clip.get().size.width} h={move || clip.get().size.height}/>
                    <div absolute overflow_hidden left={move || clip.get().origin.x} top={move || clip.get().origin.y}
                         w={move || clip.get().size.width} h={move || clip.get().size.height}>
                        <div absolute flex col overflow_hidden rounded=corners bg=Role::Surface
                             border={move || HAIRLINE * (1.0 - progress())} border_color=border test_id?=id.part("screen-container")
                             left={move || frame.get().origin.x - clip.get().origin.x}
                             top={move || frame.get().origin.y - clip.get().origin.y}
                             w={move || frame.get().size.width} h={move || frame.get().size.height}
                             pt={move || inset(|edges| edges.top)} pr={move || inset(|edges| edges.right)}
                             pb={move || inset(|edges| edges.bottom)} pl={move || inset(|edges| edges.left)}>
                            {bar}
                            <div relative flex_1 min_h=0.0 bg=Role::Canvas track_bounds=area
                                 test_id?=id.part("screen-placeholder")>
                                <web_view page exclusive absolute left=0.0 top=0.0
                                     w={move || reveal(area.get().size.width)} h={move || reveal(area.get().size.height)}
                                     page_opacity={move || morph.fade.value()}
                                     access_label={header.name.clone()} test_id?=id.part("screen-page")/>
                            </div>
                        </div>
                    </div>
                </div>
        }
    };
    view! { <overlay><show when={move || morph.shown.get()} children=screen/></overlay> }
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
    if progress <= 0.0 {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
    let mix = |from: f32, to: f32| (from + (to - from) * progress).round();
    Rect::new(
        mix(from.origin.x, to.origin.x),
        mix(from.origin.y, to.origin.y),
        mix(from.size.width, to.size.width),
        mix(from.size.height, to.size.height),
    )
}

const PAGE: Size = Size::new(960.0, 600.0);
const LEAVES: [f32; 2] = [0.0, 0.3];
const ENTERS: [f32; 2] = [0.3, 0.9];
const SCRIM_RISES: [f32; 2] = [0.0, 0.5];
const SCRIM_BLUR: f32 = 8.0;
const LIFT_Y: f32 = 12.0;
const LIFT_BLUR: f32 = 36.0;
const LIFT_ALPHA: f32 = 0.24;
const GROW: Tween = Tween::ms(300.0).ease(Ease::cubic_bezier(0.05, 0.7, 0.1, 1.0));
const SHRINK: Tween = Tween::ms(250.0).ease(Ease::cubic_bezier(0.2, 0.0, 0.0, 1.0));
const FADE_OUT: Tween = Tween::ms(90.0).ease(Ease::In);
const FADE_IN: Tween = Tween::ms(180.0).ease(Ease::Out);

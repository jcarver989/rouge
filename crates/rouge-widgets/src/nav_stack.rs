use crate::snap::PROJECTION;
use crate::tokens::Pace;
use crate::util::TestId;
use rouge_core::{
    AnyView, Apply, BoxShadow, BuildCx, DelegateStyle, Div, DragAxis, DragPhase, Key, Motion,
    MotionProp, Pose, Rect, Role, Styled, Timing, View, component, div, each, motion, route, view,
    window_handle, window_size,
};
use rouge_reactive::{Callback, Memo, Signal, memo, on_cleanup, signal};
use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavParams(Vec<(String, String)>);

impl NavParams {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct NavBack(pub(crate) Callback);

type Page = Rc<dyn Fn(NavParams) -> AnyView>;

struct Route {
    pattern: Vec<String>,
    page: Page,
}

impl Route {
    fn matches(&self, segments: &[&str]) -> Option<NavParams> {
        if self.pattern.len() != segments.len() {
            return None;
        }
        let mut params = Vec::new();
        for (pattern, segment) in self.pattern.iter().zip(segments) {
            match pattern.strip_prefix(':') {
                Some(name) => params.push((name.to_owned(), (*segment).to_owned())),
                None if pattern == segment => {}
                None => return None,
            }
        }
        Some(NavParams(params))
    }
}

#[derive(Clone, PartialEq)]
struct Entry {
    path: String,
    route: usize,
    params: NavParams,
}

pub struct NavStack {
    root: Div,
    routes: Vec<Route>,
    test_id: Option<String>,
}

#[component]
pub fn nav_stack() -> NavStack {
    NavStack {
        root: view! { <div relative overflow_hidden/> },
        routes: Vec::new(),
        test_id: None,
    }
}

impl NavStack {
    pub fn page<T: View>(mut self, pattern: &str, page: impl Fn(NavParams) -> T + 'static) -> Self {
        self.routes.push(Route {
            pattern: segments(pattern).map(str::to_owned).collect(),
            page: Rc::new(move |params| page(params).into_any()),
        });
        self
    }
}

impl NavStack {
    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }
}

impl DelegateStyle for NavStack {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.root
    }
}

fn segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|segment| !segment.is_empty())
}

fn resolve(routes: &[Route], path: &str) -> Vec<Entry> {
    let all: Vec<&str> = segments(path).collect();
    (0..=all.len())
        .filter_map(|depth| {
            let prefix = &all[..depth];
            let (route, params) = routes
                .iter()
                .enumerate()
                .find_map(|(index, route)| Some((index, route.matches(prefix)?)))?;
            Some(Entry {
                path: format!("/{}", prefix.join("/")),
                route,
                params,
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Deck {
    stack: Memo<Vec<Entry>>,
    slides: Signal<Vec<(String, Motion)>>,
    settled: Signal<bool>,
}

impl Deck {
    fn depth(self, path: &str) -> usize {
        self.stack
            .with(|stack| stack.iter().position(|entry| entry.path == path))
            .unwrap_or_default()
    }

    fn cover(self, path: &str) -> Option<Motion> {
        let above = self.stack.with(|stack| {
            let at = stack.iter().position(|entry| entry.path == path)?;
            Some(stack.get(at + 1)?.path.clone())
        })?;
        self.slides.with(|slides| {
            slides
                .iter()
                .find(|(held, _)| *held == above)
                .map(|(_, slide)| *slide)
        })
    }

    fn pop(self, path: &str) {
        let below = self.stack.with_untracked(|stack| {
            let at = stack.iter().position(|entry| entry.path == path)?;
            Some(stack.get(at.checked_sub(1)?)?.path.clone())
        });
        if let Some(below) = below {
            window_handle().navigate(below);
        }
    }

    fn screen(self, entry: &Entry, page: &Page, id: Option<String>) -> Div {
        let (path, window) = (Rc::<str>::from(entry.path.as_str()), window_size());
        let width = move || window.get_untracked().width;
        let (x, bounds, armed) = (motion(0.0), signal(Rect::ZERO), signal(false));
        let pushed = self.depth(&path) > 0;
        if pushed && self.settled.get_untracked() {
            x.set(width());
            x.animate_to(0.0, Pace::Gesture);
        }
        let slides = self.slides;
        slides.update(|slides| slides.push((path.to_string(), x)));
        on_cleanup({
            let path = path.clone();
            move || {
                if slides.is_alive() {
                    slides.update(|slides| slides.retain(|(held, _)| **held != *path));
                }
            }
        });
        let back = {
            let path = path.clone();
            move || self.pop(&path)
        };
        let covered = {
            let path = path.clone();
            move || self.cover(&path)
        };
        let shift = {
            let covered = covered.clone();
            move || match covered() {
                Some(slide) => MotionProp::Mapped(slide.map([0.0, width()], [-PARALLAX, 0.0])),
                None => MotionProp::Value(0.0),
            }
        };
        let dim = move || match covered() {
            Some(slide) => MotionProp::Mapped(slide.map([0.0, width()], [DIM, 0.0])),
            None => MotionProp::Value(0.0),
        };
        let buried = {
            let path = path.clone();
            move || self.stack.with(Vec::len) > self.depth(&path) + 2
        };
        let content = view! {
            <div size_full translate_x=shift transition=Pace::Gesture>
                {page(entry.params.clone())}
                <div absolute inset=0.0 pass_through bg=Role::Scrim opacity=dim transition=Pace::Gesture/>
            </div>
        };
        let screen = view! {
            <div test_id?=id.part(format_args!("page-{path}")) absolute inset=0.0 bg=Role::Canvas translate_x=x
                 track_bounds=bounds when=(buried, Styled::invisible)>
                {content}
            </div>
        };
        if !pushed {
            return screen;
        }
        let leave = back.clone();
        screen
            .provide(NavBack(Callback::from(back)))
            .shadow(BoxShadow::lg())
            .drag_axis(DragAxis::Horizontal)
            .on_drag(move |event| match event.phase {
                DragPhase::Start => {
                    armed.set(event.start.x - bounds.get_untracked().origin.x <= EDGE);
                }
                DragPhase::Move if armed.get_untracked() => x.set(event.translation.x.max(0.0)),
                DragPhase::End if armed.get_untracked() => {
                    let projected = x.get() + event.velocity.x * PROJECTION;
                    if projected > width() / 2.0 {
                        leave();
                    } else {
                        x.fling_to(0.0, event.velocity.x, Timing::from(Pace::Gesture));
                    }
                }
                DragPhase::Move | DragPhase::End => {}
            })
    }
}

impl View for NavStack {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (routes, id, path) = (Rc::<[Route]>::from(self.routes), self.test_id, route());
        let stack = memo({
            let routes = routes.clone();
            move || resolve(&routes, &path.get())
        });
        let deck = Deck {
            stack,
            slides: signal(Vec::new()),
            settled: signal(false),
        };
        let away = Pose::new().translate_x(window_size().get_untracked().width);
        let screens = {
            let id = id.clone();
            view! {
                <each items={move || stack.get()} key={|entry: &Entry| entry.path.clone()} let:entry
                     exit=(away, Pace::Exit)>
                    {
                        let entry = entry.get_untracked();
                        deck.screen(&entry, &routes[entry.route].page, id.clone())
                    }
                </each>
            }
        };
        self.root
            .when_some(id, Div::test_id)
            .on_key_down(move |event| {
                let top =
                    stack.with_untracked(|stack| stack.last().map(|entry| entry.path.clone()));
                if event.key == Key::Escape
                    && stack.with_untracked(Vec::len) > 1
                    && let Some(top) = top
                {
                    deck.pop(&top);
                    event.stop_propagation();
                }
            })
            .child(screens)
            .build(cx);
        deck.settled.set(true);
    }
}

const EDGE: f32 = 32.0;
const PARALLAX: f32 = 80.0;
const DIM: f32 = 0.5;

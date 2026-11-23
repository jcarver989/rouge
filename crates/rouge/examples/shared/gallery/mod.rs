#[cfg(not(target_arch = "wasm32"))]
#[path = "../camera.rs"]
mod camera;
mod controls;
mod data;
mod foundations;
mod overlays;
#[cfg(not(target_arch = "wasm32"))]
mod snapshot;
mod stage;
#[cfg(feature = "markdown")]
mod streaming;
mod touch;

use rouge::core::{
    AnyView, Appearance, Div, Rgba, Role, Styled, Theme, UiPalette, View, appearance, component,
    div, presentation, scroll, text, view,
};
use rouge::reactive::{Signal, effect, signal};
use rouge::widgets::tokens::{GRID_COLUMNS, GRID_MARGIN, Radius, Space};
use rouge::widgets::{
    CODE_FONT, InterfaceSize, Segment, Toasts, UiStyles, column_grid, grid_cell, grid_overlay,
    scroll_view, segment, segmented_control, toggle,
};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use {camera::CameraError, snapshot::snapshot};

pub(crate) fn gallery() -> impl View {
    let size = InterfaceSize::install(16.0);
    let toasts = Toasts::install();
    let dark = signal(appearance().get_untracked() == Appearance::Dark);
    let status = signal(String::from("Choose a control"));
    let grid = signal(false);
    let page = view! {
        <div flex col items_center pb=Space::X24>
            <div relative flex col gap=Space::X24 pt=Space::X24 w_full max_w=PAGE_WIDTH test_id="page">
                {stage::hero(status)}
                {foundations::foundations()}
                {foundations::materials()}
                {foundations::motion()}
                {controls::controls(status)}
                {overlays::overlays(status)}
                {data::data(size, status)}
                {streamed(status)}
                {touch::touch(status)}
                <grid_overlay visible={move || grid.get()}/>
            </div>
        </div>
    };
    size.bind(view! {
        <div size_full flex col bg=Role::Canvas>
            <header dark grid/>
            <scroll_view flex_1 min_h=0.0>
                <scroll flex_1 min_h=0.0>{page}</scroll>
            </scroll_view>
            {toasts}
        </div>
    })
}

pub(crate) const PAGE_WIDTH: f32 = 1248.0;
pub(crate) const COLUMN: f32 = 104.0;
const DEMO_MIN_HEIGHT: f32 = 64.0;
const MARK: f32 = 9.0;
const MARK_REACH: f32 = -4.0;

pub(crate) fn compact() -> impl Fn() -> bool + Copy + 'static {
    let presentation = presentation();
    move || presentation.get().is_compact()
}

#[cfg(feature = "markdown")]
fn streamed(status: Signal<String>) -> Option<Div> {
    Some(streaming::streaming(status))
}

#[cfg(not(feature = "markdown"))]
fn streamed(_status: Signal<String>) -> Option<Div> {
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Accent {
    Green,
    Blue,
    Rose,
}

impl Accent {
    const ALL: [Accent; 3] = [Accent::Green, Accent::Blue, Accent::Rose];

    fn name(self) -> &'static str {
        match self {
            Accent::Green => "Green",
            Accent::Blue => "Blue",
            Accent::Rose => "Rose",
        }
    }

    fn apply(self, palette: UiPalette, dark: bool) -> UiPalette {
        let accent = match (self, dark) {
            (Accent::Green, _) => return palette,
            (Accent::Blue, false) => 0x0a6cff,
            (Accent::Blue, true) => 0x0a84ff,
            (Accent::Rose, false) => 0xc2185b,
            (Accent::Rose, true) => 0xd6336c,
        };
        UiPalette {
            accent: Rgba::rgb(accent),
            ..palette
        }
    }
}

#[component]
fn header(dark: Signal<bool>, grid: Signal<bool>) -> impl View {
    let accent = signal(Accent::Green);
    let theme = Theme::current();
    effect(move || {
        let dark = dark.get();
        theme.set(accent.get().apply(UiPalette::for_dark(dark), dark));
    });
    let accents = Accent::ALL.map(|tint| {
        let id = format!("accent-{}", tint.name().to_lowercase());
        choice(accent, tint, tint.name(), id)
    });
    view! {
        <div relative flex justify_center bg=Role::Chrome>
            {rule().left(0.0).right(0.0).bottom(0.0).h(1.0)}
            <div flex flex_wrap items_center justify_between gap=Space::X3 px=GRID_MARGIN py=Space::X2
                 w_full max_w=PAGE_WIDTH>
                <div flex items_center gap=Space::X2>
                    <div size=12.0 flex_none rounded=Radius::Inner bg=Role::Accent/>
                    <text title primary no_wrap>"Rouge UI"</text>
                </div>
                <div flex flex_wrap items_center gap=Space::X3>
                    <div flex items_center gap=Space::X2>
                        <text small secondary>"Grid"</text>
                        <toggle checked=grid access_label="Grid" test_id="grid"/>
                    </div>
                    <segmented_control test_id="appearance">
                        <segment test_id="light" label="Light" selected={move || !dark.get()}
                                 on_click={move || dark.set(false)}/>
                        <segment test_id="dark" label="Dark" selected={move || dark.get()}
                                 on_click={move || dark.set(true)}/>
                    </segmented_control>
                    <segmented_control test_id="accent">{..accents}</segmented_control>
                </div>
            </div>
        </div>
    }
}

pub(crate) struct Cell {
    name: &'static str,
    weight: f32,
    spec: &'static str,
    note: &'static str,
    content: AnyView,
    bleed: bool,
    plain: bool,
}

pub(crate) fn cell(
    name: &'static str,
    weight: f32,
    note: &'static str,
    content: impl View,
) -> Cell {
    Cell {
        name,
        weight,
        spec: "",
        note,
        content: content.into_any(),
        bleed: false,
        plain: false,
    }
}

impl Cell {
    pub(crate) fn spec(mut self, spec: &'static str) -> Self {
        self.spec = spec;
        self
    }

    pub(crate) fn bleed(mut self) -> Self {
        self.bleed = true;
        self
    }

    pub(crate) fn plain(mut self) -> Self {
        self.plain = true;
        self
    }

    fn build(self, span: u16) -> Div {
        let frame = view! {
            <grid_cell span=span test_id={format!("cell-{}", self.name)} relative flex col>
                {rule().left(0.0).right(0.0).h(1.0)}
                {rule().top(0.0).bottom(0.0).w(1.0)}
                {..mark(Corner::TopLeft)}
                {..mark(Corner::BottomRight)}
            </grid_cell>
        };
        if self.bleed {
            return frame.px(0.0).child(self.content);
        }
        if self.plain {
            return frame.py(Space::X6).child(self.content);
        }
        let note = (!self.note.is_empty()).then(|| view! { <text small muted>{self.note}</text> });
        let (name, spec) = (self.name, self.spec);
        frame
            .py(Space::X6)
            .gap(Space::X4)
            .child(view! {
                <div flex items_center justify_between gap=Space::X2 h=20.0>
                    <text small font=CODE_FONT primary no_wrap>{name}</text>
                    <text small muted no_wrap>{spec}</text>
                </div>
            })
            .child(view! {
                <div flex col flex_1 gap=Space::X3 min_h=DEMO_MIN_HEIGHT>{self.content}</div>
            })
            .child(note)
    }
}

pub(crate) fn band(cells: impl IntoIterator<Item = Cell>) -> Vec<Div> {
    let cells: Vec<Cell> = cells.into_iter().collect();
    let total: f32 = cells.iter().map(|cell| cell.weight).sum();
    let mut left = GRID_COLUMNS;
    let last = cells.len().saturating_sub(1);
    cells
        .into_iter()
        .enumerate()
        .map(|(index, cell)| {
            let share = (f32::from(GRID_COLUMNS) * cell.weight / total).round() as u16;
            let span = if index == last { left } else { share.min(left) };
            left -= span;
            cell.build(span)
        })
        .collect()
}

pub(crate) fn section(
    title: &'static str,
    detail: &'static str,
    bands: impl IntoIterator<Item = Vec<Div>>,
) -> Div {
    let heading = view! {
        <column_grid>
            <grid_cell span=4>
                <text text_size=32.0 line_height=1.25 weight=600.0 primary>{title}</text>
            </grid_cell>
            <grid_cell span=7 pt=Space::X2>
                <text reading secondary>{detail}</text>
            </grid_cell>
        </column_grid>
    };
    view! {
        <div flex col gap=Space::X8 test_id={format!("section-{}", title.to_lowercase())}>
            {heading}
            {sheet(bands.into_iter().flatten())}
        </div>
    }
}

pub(crate) fn sheet(cells: impl IntoIterator<Item = Div>) -> Div {
    view! {
        <column_grid relative>
            {..cells}
            {rule().left(0.0).right(0.0).bottom(-1.0).h(1.0)}
            {rule().top(0.0).bottom(0.0).right(-1.0).w(1.0)}
            {..mark(Corner::TopRight)}
            {..mark(Corner::BottomLeft)}
        </column_grid>
    }
}

#[derive(Clone, Copy)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

fn rule() -> Div {
    view! { <div absolute bg=Role::BorderSubtle pass_through/> }
}

fn mark(corner: Corner) -> [Div; 2] {
    let stroke = || view! { <div absolute bg=Role::TextMuted pass_through/> };
    let (across, down) = (stroke().w(MARK).h(1.0), stroke().w(1.0).h(MARK));
    match corner {
        Corner::TopLeft => [
            across.left(MARK_REACH).top(0.0),
            down.left(0.0).top(MARK_REACH),
        ],
        Corner::TopRight => [
            across.right(MARK_REACH - 1.0).top(0.0),
            down.right(-1.0).top(MARK_REACH),
        ],
        Corner::BottomLeft => [
            across.left(MARK_REACH).bottom(-1.0),
            down.left(0.0).bottom(MARK_REACH - 1.0),
        ],
        Corner::BottomRight => [
            across.right(MARK_REACH - 1.0).bottom(-1.0),
            down.right(-1.0).bottom(MARK_REACH - 1.0),
        ],
    }
}

pub(crate) fn say(status: Signal<String>, message: &'static str) -> impl Fn() + Copy {
    move || status.set(message.into())
}

pub(crate) fn choice<T: PartialEq + Copy + 'static>(
    group: Signal<T>,
    value: T,
    label: &str,
    id: String,
) -> Segment {
    view! {
        <segment label=label.to_owned() test_id=id selected={move || group.get() == value}
             on_click={move || group.set(value)}/>
    }
}

#[component]
pub(crate) fn row() -> Div {
    view! { <div flex flex_wrap items_center gap=Space::X2/> }
}

#[component]
pub(crate) fn column() -> Div {
    view! { <div flex col gap=Space::X2/> }
}

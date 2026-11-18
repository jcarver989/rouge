mod fuzzy;
mod state;

use crate::{
    components::{ListRow, list_row, muted_text},
    icon::{Icon, IconName},
    material::{Material, MaterialExt},
    tokens::{self, Space},
    typography::UiStyles,
    util::{Spot, TestId, chrome},
};
use rouge_core::Role;
use rouge_core::{
    AnyView, BuildCx, Color, DelegateStyle, Div, Fill, IntoProp, Key, Length, Prop, RowKey,
    ScrollState, Scrollable, StyleDelta, Styled, View, component, div, dynamic, editable, pct, px,
    scroll, show, text, view, virtual_list, virtual_list_handle,
};
use rouge_reactive::{Callback, Memo, Signal, batch, effect, memo, signal};
use rouge_theme::SelectionState;
use state::{PickerRow, PickerState};
use std::mem;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickerMarker {
    Dot(Color),
    Pending,
}

impl View for PickerMarker {
    fn build(self, cx: &mut BuildCx<'_>) {
        match self {
            PickerMarker::Dot(color) => view! { <div size=MARKER_SIZE rounded=tokens::Radius::Full bg=color/> },
            PickerMarker::Pending => view! {
                <div size=MARKER_SIZE rounded=tokens::Radius::Full border=1.0 border_color=Role::TextMuted/>
            },
        }
        .build(cx);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickerItem {
    id: String,
    label: String,
    marker: Option<PickerMarker>,
    icon: Option<IconName>,
    group: Option<String>,
    prefix: Option<String>,
    subtitle: Option<String>,
    details: Vec<String>,
    keywords: Option<String>,
    disabled: bool,
    checked: bool,
    search: String,
}

impl PickerItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            marker: None,
            icon: None,
            group: None,
            prefix: None,
            subtitle: None,
            details: Vec::new(),
            keywords: None,
            disabled: false,
            checked: false,
            search: String::new(),
        }
        .indexed()
    }

    pub const fn marker(mut self, marker: PickerMarker) -> Self {
        self.marker = Some(marker);
        self
    }

    pub const fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self.indexed()
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self.indexed()
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.details.push(detail.into());
        self.indexed()
    }

    pub fn keywords(mut self, keywords: impl Into<String>) -> Self {
        self.keywords = Some(keywords.into());
        self.indexed()
    }

    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub const fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    fn indexed(mut self) -> Self {
        self.search.clear();
        let parts = [
            self.prefix.as_ref(),
            Some(&self.label),
            self.subtitle.as_ref(),
        ]
        .into_iter()
        .flatten()
        .chain(&self.details)
        .chain(self.keywords.as_ref());
        for (index, part) in parts.enumerate() {
            if index > 0 {
                self.search.push(' ');
            }
            self.search.push_str(part);
        }
        self
    }
}

pub struct PickerList {
    items: Vec<PickerItem>,
    highlighted: Prop<Option<usize>>,
    empty: Option<String>,
    scroll: Option<Signal<ScrollState>>,
    on_choose: Callback<(usize,)>,
    on_hover: Callback<(usize,)>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn picker_list(items: Vec<PickerItem>) -> PickerList {
    PickerList {
        items,
        highlighted: Prop::Static(None),
        empty: None,
        scroll: None,
        on_choose: Callback::default(),
        on_hover: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl PickerList {
    pub fn highlighted<T>(mut self, highlighted: impl IntoProp<Option<usize>, T>) -> Self {
        self.highlighted = highlighted.into_prop();
        self
    }

    pub fn empty(mut self, empty: impl Into<String>) -> Self {
        self.empty = Some(empty.into());
        self
    }

    pub fn track_scroll(mut self, state: Signal<ScrollState>) -> Self {
        self.scroll = Some(state);
        self
    }

    pub fn on_choose(mut self, on_choose: impl Into<Callback<(usize,)>>) -> Self {
        self.on_choose = on_choose.into();
        self
    }

    pub fn on_hover(mut self, on_hover: impl Into<Callback<(usize,)>>) -> Self {
        self.on_hover = on_hover.into();
        self
    }
}

chrome!(PickerList);

impl View for PickerList {
    fn build(self, cx: &mut BuildCx<'_>) {
        let PickerList {
            items,
            highlighted,
            empty,
            scroll: state,
            on_choose,
            on_hover,
            test_id,
            style,
        } = self;
        let state = state.unwrap_or_else(|| signal(ScrollState::default()));
        let columns = Columns::new(&items);
        let picker = PickerState::new(items).unfiltered();
        let highlighted = Rc::new(highlighted);
        let spot = Spot::new();
        let list = if picker.options().is_empty() {
            view! { <div><muted_text>{empty.unwrap_or_else(|| "No results".into())}</muted_text></div> }
        } else {
            let rows = picker.rows().into_iter().map(|row| {
                let index = row.index();
                let highlighted = highlighted.clone();
                entry(
                    row,
                    &picker.options()[index],
                    &columns,
                    move || highlighted.get() == Some(index),
                    spot,
                    Callback::from(move || on_choose.call(index)),
                    Callback::from(move || on_hover.call(index)),
                )
            });
            view! { <div relative flex col>{spotlight(spot)}{..rows}</div> }
        };
        let root = view! {
            <scroll test_id?=test_id max_h=px(tokens::PICKER_LIST_HEIGHT) scroll_state=state>
                {list}
            </scroll>
        };
        style.apply(root).build(cx);
    }
}

fn group(item: &PickerItem) -> impl View {
    view! {
        <div px=Space::X2 pt=Space::X2><text caption muted>{item.group.clone().unwrap_or_default()}</text></div>
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Columns {
    marker: bool,
    checked: bool,
    prefix: Option<String>,
}

impl Columns {
    fn new(items: &[PickerItem]) -> Self {
        Self {
            marker: items.iter().any(|item| item.marker.is_some()),
            checked: items.iter().any(|item| item.checked),
            prefix: items
                .iter()
                .filter_map(|item| item.prefix.as_ref())
                .max_by_key(|prefix| prefix.chars().count())
                .cloned(),
        }
    }
}

fn entry(
    row: PickerRow,
    item: &PickerItem,
    columns: &Columns,
    highlighted: impl Fn() -> bool + 'static,
    spot: Spot,
    choose: Callback,
    hover: Callback,
) -> AnyView {
    if let PickerRow::Group(_) = row {
        return group(item).into_any();
    }
    let disabled = item.disabled;
    let state = if disabled {
        SelectionState::Disabled
    } else {
        SelectionState::None
    };
    let selection = memo(move || {
        if highlighted() {
            SelectionState::Selected
        } else {
            SelectionState::None
        }
    });
    spot.track(view! { <div/> }, move || {
        selection.get() == SelectionState::Selected
    })
    .on_pointer_move(move |_| {
        if !disabled {
            hover.call();
        }
    })
    .child(item_row(item, columns, state, selection, choose))
    .into_any()
}

fn spotlight(spot: Spot) -> Div {
    let layer = view! { <div absolute inset=0.0 pass_through/> };
    spot.contain(layer).child(spot.follower(|layer| {
        layer
            .test_id("picker-highlight")
            .material(Material::Pigment)
            .rounded(tokens::Radius::Inner)
    }))
}

fn item_row(
    item: &PickerItem,
    columns: &Columns,
    state: SelectionState,
    selection: Memo<SelectionState>,
    on_click: Callback,
) -> ListRow {
    let ink = move |role: Role| {
        move || {
            Fill::Role(if selection.get() == SelectionState::Selected {
                Role::AccentForeground
            } else {
                role
            })
        }
    };
    let slot =
        || view! { <div flex_none size=tokens::ICON_SIZE flex items_center justify_center/> };
    let marker = columns.marker.then(|| {
        slot()
            .test_id(format!("{}-marker", item.id))
            .child(item.marker)
    });
    let prefix = columns.prefix.as_deref().map(|widest| {
        let code = |value: &str| view! { <text code color={ink(Role::TextMuted)} no_wrap>{value.to_owned()}</text> };
        view! {
            <div relative flex_none>
                <div opacity=0.0 pass_through>{code(widest)}</div>
                {item.prefix.as_deref().map(|value| view! { <div absolute left=0.0 top=0.0>{code(value)}</div> })}
            </div>
        }
    });
    let subtitle = item.subtitle.as_deref().map(|subtitle| {
        view! {
            <div test_id={format!("{}-subtitle", item.id)}>
                <text small color={ink(Role::TextMuted)} no_wrap>{subtitle.to_owned()}</text>
            </div>
        }
    });
    let details = (!item.details.is_empty()).then(|| {
        let details = item.details.iter().map(|detail| {
            view! { <text caption color={ink(Role::TextMuted)} no_wrap>{detail.clone()}</text> }
        });
        view! { <div flex items_center gap=Space::X2>{..details}</div> }
    });
    let check = columns.checked.then(|| {
        let icon = item
            .checked
            .then(|| Icon::new(IconName::Check, ink(Role::Accent)).size(12.0));
        slot().test_id(format!("{}-check", item.id)).child(icon)
    });
    view! {
        <list_row test_id={item.id.clone()} on_click=on_click selection=state>
            {marker}
            {item.icon.map(|icon| Icon::new(icon, ink(Role::TextSecondary)))}
            {prefix}
            <div flex col flex_1 min_w=0.0 overflow_hidden when:py=(subtitle.is_some(), 4.0)>
                <div test_id={format!("{}-label", item.id)}>
                    <text body color={ink(Role::Text)} no_wrap>{item.label.clone()}</text>
                </div>
                {subtitle}
            </div>
            {details}
            {check}
        </list_row>
    }
    .hover(|style| style.bg(Color::TRANSPARENT))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchPickerEvent {
    Query(String),
    Highlighted(usize),
    Chose(usize),
    Dismissed,
}

#[derive(Clone)]
pub struct Preview(Rc<dyn Fn(usize) -> AnyView>);

impl<T: Fn(usize) -> AnyView + 'static> From<T> for Preview {
    fn from(render: T) -> Self {
        Self(Rc::new(render))
    }
}

#[derive(Clone, Copy)]
pub struct SearchPickerHandle {
    model: Signal<PickerState>,
    revision: Signal<u64>,
    query: Signal<String>,
    emit: Callback<(SearchPickerEvent,)>,
}

impl SearchPickerHandle {
    pub fn new(items: Vec<PickerItem>, emit: impl Fn(SearchPickerEvent) + 'static) -> Self {
        Self {
            model: signal(PickerState::new(items)),
            revision: signal(0),
            query: signal(String::new()),
            emit: emit.into(),
        }
    }

    pub fn unfiltered(self) -> Self {
        self.configure(PickerState::unfiltered)
    }

    pub fn max_matches(self, limit: usize) -> Self {
        self.configure(|model| model.limit(limit))
    }

    pub fn highlighted(self, index: usize) -> Self {
        self.model.update(|model| {
            model.highlight(index);
        });
        self
    }

    pub fn set_items(&self, items: Vec<PickerItem>) {
        batch(|| {
            self.model.update(|model| model.set_options(items));
            self.revision.update(|revision| *revision += 1);
        });
    }

    pub fn query(&self) -> Signal<String> {
        self.query
    }

    pub fn move_highlight(&self, by: isize) {
        self.update(|model| {
            model.move_by(by);
        });
    }

    pub fn highlight(&self, index: usize) {
        self.update(|model| {
            model.highlight(index);
        });
    }

    pub fn choose(&self) {
        if let Some(index) = self.model.with_untracked(PickerState::highlighted) {
            self.emit.call(SearchPickerEvent::Chose(index));
        }
    }

    pub fn dismiss(&self) {
        self.emit.call(SearchPickerEvent::Dismissed);
    }

    fn update(&self, f: impl FnOnce(&mut PickerState)) {
        let before = self.model.with_untracked(PickerState::highlighted);
        self.model.update(f);
        if let Some(after) = self
            .model
            .with_untracked(PickerState::highlighted)
            .filter(|after| Some(*after) != before)
        {
            self.emit.call(SearchPickerEvent::Highlighted(after));
        }
    }

    fn configure(self, f: impl FnOnce(PickerState) -> PickerState) -> Self {
        self.model
            .update(|model| *model = f(mem::replace(model, PickerState::new(Vec::new()))));
        self
    }
}

pub struct SearchPicker {
    handle: SearchPickerHandle,
    searchable: Option<String>,
    title: Option<String>,
    footer: Option<String>,
    empty: Option<String>,
    max_list_height: Option<Length>,
    preview: Option<Preview>,
    status: Option<Signal<Option<String>>>,
    material: Material,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn search_picker(handle: SearchPickerHandle) -> SearchPicker {
    SearchPicker {
        handle,
        searchable: None,
        title: None,
        footer: None,
        empty: None,
        max_list_height: None,
        preview: None,
        status: None,
        material: Material::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl SearchPicker {
    pub fn searchable(mut self, hint: impl Into<String>) -> Self {
        self.searchable = Some(hint.into());
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    pub fn empty(mut self, empty: impl Into<String>) -> Self {
        self.empty = Some(empty.into());
        self
    }

    pub fn max_list_height(mut self, height: impl Into<Length>) -> Self {
        self.max_list_height = Some(height.into());
        self
    }

    pub fn preview(mut self, preview: impl Into<Preview>) -> Self {
        self.preview = Some(preview.into());
        self
    }

    pub fn status(mut self, status: Signal<Option<String>>) -> Self {
        self.status = Some(status);
        self
    }

    pub fn material(mut self, material: Material) -> Self {
        self.material = material;
        self
    }
}

chrome!(SearchPicker);

impl View for SearchPicker {
    fn build(self, cx: &mut BuildCx<'_>) {
        let style = self.style;
        let root = search_root(
            self.test_id,
            self.handle,
            self.searchable,
            self.title,
            self.footer,
            self.empty,
            self.max_list_height,
            self.preview,
            self.status,
        )
        .material(self.material);
        style.apply(root).build(cx);
    }
}

#[allow(clippy::too_many_arguments)]
fn search_root(
    id: Option<String>,
    handle: SearchPickerHandle,
    searchable: Option<String>,
    title: Option<String>,
    footer: Option<String>,
    empty: Option<String>,
    max_list_height: Option<Length>,
    preview: Option<Preview>,
    status: Option<Signal<Option<String>>>,
) -> Div {
    let (query, model) = (handle.query, handle.model);
    effect(move || {
        let next = query.get();
        if !model.with_untracked(|model| model.query() == next) {
            handle.emit.call(SearchPickerEvent::Query(next.clone()));
            handle.update(|model| model.set_query(next));
        }
    });
    let highlight = memo(move || model.with(PickerState::highlighted));
    let entries = memo(move || model.with(PickerState::rows));
    let focus_root = searchable.is_none();
    let empty = empty.unwrap_or_else(|| "No matching items".into());
    let list_height = max_list_height.unwrap_or(px(tokens::PICKER_LIST_HEIGHT));
    let list = results(handle, entries, highlight, empty, list_height);
    let status = status.unwrap_or_else(|| signal(None));
    let list = match preview {
        Some(preview) => {
            let preview_id = id.part("preview");
            view! { <with_preview id?=preview_id highlight preview>{list}</with_preview> }
                .into_any()
        }
        None => list.into_any(),
    };
    view! {
        <div test_id?=id flex col min_h=0.0 w=px(tokens::PICKER_WIDTH)
             max_w=pct(100.0) gap=Space::X2 p=Space::X2 when:autofocus=focus_root
             when:tab_index=(focus_root, -1)
             on_key_down={move |event| {
                 match event.key {
                     Key::ArrowUp => handle.move_highlight(-1),
                     Key::ArrowDown => handle.move_highlight(1),
                     Key::PageUp => handle.move_highlight(-8),
                     Key::PageDown => handle.move_highlight(8),
                     Key::Enter => handle.choose(),
                     Key::Escape => handle.dismiss(),
                     _ => return,
                 }
                 event.stop_propagation();
             }}>
            {title.map(|title| view! { <text title primary>{title}</text> })}
            {searchable.map(|hint| view! { <query_input hint handle/> })}
            {list}
            <show when={move || status.with(Option::is_some)}>
                <text caption color=Role::Warning>{move || status.get().unwrap_or_default()}</text>
            </show>
            {footer.map(|footer| view! { <muted_text>{footer}</muted_text> })}
        </div>
    }
}

#[component]
fn with_preview(
    id: Option<String>,
    children: impl View,
    highlight: Memo<Option<usize>>,
    preview: Preview,
) -> impl View {
    view! {
        <div flex row flex_1 min_h=0.0>
            <div flex_none w=tokens::PICKER_PREVIEW_LIST_WIDTH max_w=pct(50.0) min_h=0.0 pr=Space::X2 border_r=1.0
                 border_color=Role::BorderSubtle flex col>
                {children}
            </div>
            <div test_id?=id flex_1 min_w=0.0 min_h=0.0 flex col>
                <dynamic>{highlight.get().map(|index| (preview.0)(index))}</dynamic>
            </div>
        </div>
    }
}

#[component]
fn query_input(hint: String, handle: SearchPickerHandle) -> impl View {
    let query = handle.query;
    view! {
        <div h=tokens::GROUP_HEIGHT px=Space::X2 flex items_center material=Material::Surface rounded=tokens::Radius::Inner>
            <editable value=query test_id="picker-query" autofocus body color=Role::Text w_full
                      placeholder=(hint, Role::TextMuted) on_submit={move |_| handle.choose()}/>
        </div>
    }
}

fn results(
    handle: SearchPickerHandle,
    entries: Memo<Vec<PickerRow>>,
    highlight: Memo<Option<usize>>,
    empty: String,
    list_height: Length,
) -> impl View {
    let (model, revision) = (handle.model, handle.revision);
    let columns = memo(move || model.with(|model| Columns::new(model.options())));
    let list = virtual_list_handle();
    let spot = Spot::new();
    effect(move || {
        let target = highlight.get();
        let row = entries.with(|rows| {
            rows.iter()
                .position(|row| Some(*row) == target.map(PickerRow::Option))
        });
        if let Some(row) = row {
            list.reveal_row(row, 0.0);
        }
    });
    let rows = view! {
        <virtual_list count={move || entries.with(Vec::len)} row_height=tokens::ROW_HEIGHT let:position
             row_identity={move |position| RowKey::new(entries.with(|rows| rows[position]), revision.get())}
             measure_rows={|| 0} underlay=spotlight(spot) handle=list max_h=list_height>
            {
                let row = entries.with_untracked(|rows| rows[position]);
                let index = row.index();
                let choose = Callback::from(move || {
                    handle.highlight(index);
                    handle.choose();
                });
                let hover = Callback::from(move || handle.highlight(index));
                let highlighted = move || highlight.get() == Some(index);
                columns.with_untracked(|columns| {
                    model.with_untracked(|model| {
                        let item = &model.options()[index];
                        entry(row, item, columns, highlighted, spot, choose, hover)
                    })
                })
            }
        </virtual_list>
    };
    view! {
        <div test_id="picker-results" flex col min_h=0.0>
            <show when={move || entries.with(Vec::is_empty)}><muted_text>{empty.clone()}</muted_text></show>
            {rows}
        </div>
    }
}

const MARKER_SIZE: f32 = 8.0;

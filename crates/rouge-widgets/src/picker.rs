use crate::{
    components::{list_row, muted_text},
    icon::{Icon, IconName},
    material::{Material, MaterialExt},
    tokens,
    typography::UiStyles,
};
use rouge_core::{
    AnyView, BuildCx, Color, Div, ElementId, Key, Length, RowKey, ScrollState, Scrollable, Styled,
    Theme, View, component, div, dynamic, editable, pct, px, scroll, show, text, view,
    virtual_list, virtual_list_handle,
};
use rouge_reactive::{Callback, Memo, Signal, batch, effect, memo, signal};
use rouge_theme::{PickerEntry, PickerRow, PickerState, SelectionState};
use std::borrow::Cow;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickerMarker {
    Dot(Color),
    Pending,
}

impl View for PickerMarker {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        match self {
            PickerMarker::Dot(color) => view! { <div size=7.0 rounded=4.0 bg=color/> },
            PickerMarker::Pending => view! {
                <div size=10.0 rounded=5.0 border=px(1.0) border_color=Theme::current().text_muted/>
            },
        }
        .build(cx)
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
        }
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
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.details.push(detail.into());
        self
    }

    pub fn keywords(mut self, keywords: impl Into<String>) -> Self {
        self.keywords = Some(keywords.into());
        self
    }

    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub const fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

impl PickerEntry for PickerItem {
    fn text(&self) -> Cow<'_, str> {
        let text = [
            self.prefix.as_ref(),
            Some(&self.label),
            self.subtitle.as_ref(),
        ]
        .into_iter()
        .flatten()
        .chain(&self.details)
        .chain(self.keywords.as_ref())
        .map(String::as_str)
        .collect::<Vec<&str>>()
        .join(" ");
        Cow::Owned(text)
    }

    fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }

    fn disabled(&self) -> bool {
        self.disabled
    }
}

#[component]
#[allow(clippy::needless_pass_by_value)]
pub fn picker_list(
    #[prop(into)] id: String,
    items: Vec<PickerItem>,
    highlighted: Option<usize>,
    #[prop(into)] empty: Option<String>,
    max_height: Option<Length>,
    track_scroll: Option<Signal<ScrollState>>,
    #[prop(default, into)] on_choose: Callback<(usize,)>,
    #[prop(default, into)] on_hover: Callback<(usize,)>,
) -> impl View {
    let state = track_scroll.unwrap_or_else(|| signal(ScrollState::default()));
    let columns = Columns::new(&items);
    let picker = PickerState::new(items).unfiltered();
    let list = if picker.options().is_empty() {
        view! { <div><muted_text>{empty.unwrap_or_else(|| "No results".into())}</muted_text></div> }
    } else {
        let rows = picker.rows().into_iter().map(|row| {
            let index = row.index();
            entry(
                row,
                picker.options()[index].clone(),
                &columns,
                move || highlighted == Some(index),
                Callback::from(move || on_choose.call(index)),
                Callback::from(move || on_hover.call(index)),
            )
        });
        view! { <div flex col>{..rows}</div> }
    };
    view! {
        <scroll test_id=id max_h={max_height.unwrap_or_else(|| Length::design(tokens::PICKER_LIST_HEIGHT))}
                scroll_state=state>
            {list}
        </scroll>
    }
}

fn group(item: &PickerItem) -> impl View {
    view! {
        <div px=8.0 pt=6.0><text caption muted>{item.group.clone().unwrap_or_default()}</text></div>
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
    item: PickerItem,
    columns: &Columns,
    highlighted: impl Fn() -> bool + 'static,
    choose: Callback,
    hover: Callback,
) -> AnyView {
    if let PickerRow::Group(_) = row {
        return group(&item).into_any();
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
    item_row(item, columns, state, selection, choose)
        .on_pointer_move(move |_| {
            if !disabled {
                hover.call();
            }
        })
        .into_any()
}

fn item_row(
    item: PickerItem,
    columns: &Columns,
    state: SelectionState,
    selection: Memo<SelectionState>,
    on_click: Callback,
) -> Div {
    let theme = Theme::current();
    let slot =
        || view! { <div flex_none size=tokens::ICON_SIZE flex items_center justify_center/> };
    let marker = columns.marker.then(|| {
        slot()
            .test_id(format!("{}-marker", item.id))
            .child(item.marker)
    });
    let prefix = columns.prefix.clone().map(|widest| {
        let code = |value: String| view! { <text code muted no_wrap>{value}</text> };
        view! {
            <div relative flex_none>
                <div opacity=0.0 pass_through>{code(widest)}</div>
                {item.prefix.map(|value| view! { <div absolute left=0.0 top=0.0>{code(value)}</div> })}
            </div>
        }
    });
    let subtitle = item.subtitle.map(|subtitle| {
        view! {
            <div test_id={format!("{}-subtitle", item.id)}><text small muted no_wrap>{subtitle}</text></div>
        }
    });
    let details = (!item.details.is_empty()).then(|| {
        let details = item.details.into_iter().map(move |detail| {
            view! { <text caption muted no_wrap>{detail}</text> }
        });
        view! { <div flex items_center gap=6.0>{..details}</div> }
    });
    let check = columns.checked.then(|| {
        let icon = item
            .checked
            .then(|| Icon::new(IconName::Check, theme.accent).size(12.0));
        slot().test_id(format!("{}-check", item.id)).child(icon)
    });
    view! {
        <list_row id={item.id.clone()} state selection on_click>
            {marker}
            {item.icon.map(|icon| Icon::new(icon, theme.text_secondary))}
            {prefix}
            <div flex col flex_1 min_w=0.0 overflow_hidden when:py=(subtitle.is_some(), 4.0)>
                <div test_id={format!("{}-label", item.id)}><text body no_wrap>{item.label}</text></div>
                {subtitle}
            </div>
            {details}
            {check}
        </list_row>
    }
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
    model: Signal<PickerState<PickerItem>>,
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
        self.model
            .update(|model| *model = model.clone().unfiltered());
        self
    }

    pub fn max_matches(self, limit: usize) -> Self {
        self.model
            .update(|model| *model = model.clone().limit(limit));
        self
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

    fn update(&self, f: impl FnOnce(&mut PickerState<PickerItem>)) {
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

    fn item(&self, index: usize) -> PickerItem {
        self.model
            .with_untracked(|model| model.options()[index].clone())
    }
}

#[component]
pub fn search_picker(
    #[prop(into)] id: String,
    handle: SearchPickerHandle,
    #[prop(into)] searchable: Option<String>,
    #[prop(into)] title: Option<String>,
    #[prop(into)] footer: Option<String>,
    #[prop(into)] empty: Option<String>,
    width: Option<Length>,
    height: Option<Length>,
    max_list_height: Option<Length>,
    #[prop(into)] preview: Option<Preview>,
    status: Option<Signal<Option<String>>>,
    #[prop(default)] material: Material,
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
    let list_height = max_list_height.unwrap_or_else(|| Length::design(tokens::PICKER_LIST_HEIGHT));
    let list = results(handle, entries, highlight, empty, list_height);
    let status = status.unwrap_or_else(|| signal(None));
    let list = match preview {
        Some(preview) => {
            view! { <with_preview id={&id} highlight preview>{list}</with_preview> }.into_any()
        }
        None => list.into_any(),
    };
    let root = view! {
        <div test_id=id flex col min_h=0.0 w={width.unwrap_or_else(|| Length::design(tokens::PICKER_WIDTH))}
             max_w=pct(100.0) gap=6.0 p=8.0 h?=height when:autofocus=focus_root
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
                <text caption color=Theme::current().warning>{move || status.get().unwrap_or_default()}</text>
            </show>
            {footer.map(|footer| view! { <muted_text>{footer}</muted_text> })}
        </div>
    };
    root.material(material)
}

#[component]
fn with_preview(
    id: &str,
    children: impl View,
    highlight: Memo<Option<usize>>,
    preview: Preview,
) -> impl View {
    view! {
        <div flex row flex_1 min_h=0.0>
            <div flex_none w=tokens::PICKER_PREVIEW_LIST_WIDTH max_w=pct(50.0) min_h=0.0 pr=6.0 border_r=px(1.0)
                 border_color=Theme::current().border_subtle>
                {children}
            </div>
            <div test_id={format!("{id}-preview")} flex_1 min_w=0.0 min_h=0.0 flex col>
                <dynamic>{highlight.get().map(|index| (preview.0)(index))}</dynamic>
            </div>
        </div>
    }
}

#[component]
fn query_input(hint: String, handle: SearchPickerHandle) -> impl View {
    let query = handle.query;
    view! {
        <div p=6.0 material=Material::Surface>
            <editable value=query test_id="picker-query" autofocus body color=Theme::current().text w_full
                      placeholder=(hint, Theme::current().text_muted) on_submit={move |_| handle.choose()}/>
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
    let rows = virtual_list(
        move || entries.with(Vec::len),
        tokens::ROW_HEIGHT,
        move |position| {
            let row = entries.with_untracked(|rows| rows[position]);
            let index = row.index();
            entry(
                row,
                handle.item(index),
                &columns.get_untracked(),
                move || highlight.get() == Some(index),
                Callback::from(move || {
                    handle.highlight(index);
                    handle.choose();
                }),
                Callback::from(move || handle.highlight(index)),
            )
        },
    )
    .row_identity(move |position| RowKey::new(entries.with(|rows| rows[position]), revision.get()))
    .measure_rows(|| 0)
    .handle(list)
    .max_h(list_height);
    view! {
        <div test_id="picker-results" flex col min_h=0.0>
            <show when={move || entries.with(Vec::is_empty)}><muted_text>{empty.clone()}</muted_text></show>
            {rows}
        </div>
    }
}

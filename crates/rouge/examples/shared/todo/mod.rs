mod model;
mod rings;
mod tokens;

use model::{Filter, Todos};
use rings::{Touch, ring_field};
use rouge::core::presentation;
use rouge::prelude::*;
use rouge::widgets::tokens::{
    CONTROL_HEIGHT_LARGE, FLOATING_INSET, HAIRLINE, ICON_BUTTON_SIZE, Pace, Radius, Space,
};
use rouge::widgets::{
    CheckModel, CheckState, ControlSize, IconName, UiStyles, icon_button, inline_edit, kbd,
    list_row, row_action, segment, segmented_control,
};
use tokens::{
    CARD_WIDTH, EDITOR_HEIGHT, ROW_ENTER, ROW_EXIT, TABS_HEIGHT, TITLE_LINE_HEIGHT, TITLE_SIZE,
};

pub(crate) fn todo_app() -> impl View {
    let appearance = appearance();
    let palette = move || UiPalette::for_dark(appearance.get() == Appearance::Dark);
    let theme = Theme::provide(untrack(palette));
    effect(move || theme.set(palette()));
    let todos = Todos::new();
    let has_todos = move || !todos.items.is_empty();
    let touch = stored(Touch::Unused);
    let card = signal(Rect::ZERO);
    view! {
        <div size_full relative on_pointer_down={move |event: &PointerEvent| touch.set(Touch::Press(event.position))}>
            <ring_field todos touch card/>
            <scroll_view size_full>
                <scroll>
                    <div flex col items_center w_full px=Space::X4 pt=Space::X12 pb=Space::X12>
                        <text w_full max_w=CARD_WIDTH display text_size=TITLE_SIZE line_height=TITLE_LINE_HEIGHT primary>"todos"</text>
                        <div flex col w_full max_w=CARD_WIDTH mt=Space::X6 material=Material::Clear track_bounds=card test_id="card">
                            <new_todo todos touch/>
                            <show when=has_todos>
                                <todo_list todos/>
                                <footer todos/>
                            </show>
                        </div>
                        <show when=has_todos>
                            <hint/>
                        </show>
                    </div>
                </scroll>
            </scroll_view>
        </div>
    }
}

#[component]
fn new_todo(todos: Todos, touch: Stored<Touch>) -> impl View {
    let accent = Theme::current().get_untracked().accent;
    let draft = signal(String::new());
    let add = move |title: &str| {
        touch.set(Touch::Card);
        todos.add(title);
        draft.set(String::new());
    };
    view! {
        <div flex items_center gap=Space::X3 px=Space::X4 py=Space::X4>
            <div flex flex_none items_center justify_center size=ICON_BUTTON_SIZE>
                <show when={move || !todos.items.is_empty()}>
                    <icon_button icon=IconName::ChevronDown label="Toggle all"
                                 selected={move || todos.remaining.get() == 0}
                                 on_click={move || todos.toggle_all()} test_id="toggle-all"/>
                </show>
            </div>
            <div flex_1 min_w=0.0>
                <editable value=draft w_full reading color=Role::Text caret_color=accent
                          placeholder=("What needs to be done?", Role::TextMuted)
                          autofocus on_submit=add test_id="new-todo"/>
            </div>
        </div>
    }
}

#[component]
fn todo_list(todos: Todos) -> impl View {
    view! {
        <div flex col p=FLOATING_INSET border_t=HAIRLINE border_color=Role::BorderSubtle>
            <each items=todos.visible key={|id| *id} let:id
                  enter=(ROW_ENTER, Pace::Layout) exit=(ROW_EXIT, Pace::Exit)>
                <todo_row todos id={id.get_untracked()}/>
            </each>
        </div>
    }
}

#[component]
fn todo_row(todos: Todos, id: u64) -> impl View {
    let done = todos.items.select(id, |todo| todo.done);
    let title = todos.items.select(id, |todo| todo.title.clone());
    let ink = done.pick(Fill::Role(Role::TextMuted), Fill::Role(Role::Text));
    let delete = row_action("Delete", move || {
        todos.items.remove(&id);
    })
    .icon(IconName::X)
    .destructive()
    .test_id("delete");
    view! {
        <div layout_transition=Pace::Layout test_scope={format!("todo-{id}")}>
            <list_row h=CONTROL_HEIGHT_LARGE px=Space::X2 gap=Space::X3 action=delete>
                <div flex flex_none items_center justify_center size=ICON_BUTTON_SIZE>
                    <checkbox checked={Done(todos, id)} test_id="toggle"/>
                </div>
                <inline_edit value=title flex_1 min_w=0.0 reading text_color=ink strikethrough=done
                             editing:flex editing:items_center editing:h=EDITOR_HEIGHT editing:px=Space::X2
                             editing:ml={-(Space::X2.px() + HAIRLINE)} editing:rounded=Radius::Control
                             on_commit={move |title: String| todos.rename(id, &title)} test_id="title"/>
            </list_row>
        </div>
    }
}

#[component]
fn footer(todos: Todos) -> impl View {
    let presentation = presentation();
    let compact = move || presentation.get().is_compact();
    let edge = Space::X3.px();
    let items_left = move || match todos.remaining.get() {
        1 => "1 item left".to_owned(),
        count => format!("{count} items left"),
    };
    view! {
        <div relative flex justify_center px=Space::X4 pt=edge border_t=HAIRLINE border_color=Role::BorderSubtle
             pb={move || if compact() { edge + Space::X3.px() + TABS_HEIGHT } else { edge }}>
            <div absolute left=Space::X4 right=Space::X4 bottom=edge h=TABS_HEIGHT flex items_center justify_between>
                <text small muted test_id="items-left">{items_left}</text>
                <show when={move || todos.remaining.get() < todos.items.len()}>
                    <button label="Clear completed" variant=ButtonVariant::Ghost control_size=ControlSize::Small
                            on_click={move || todos.items.retain(|todo| !todo.done)} test_id="clear-completed"/>
                </show>
            </div>
            <filter_tabs todos/>
        </div>
    }
}

#[component]
fn filter_tabs(todos: Todos) -> impl View {
    let window = window_handle();
    let tabs = Filter::ALL.map(|filter| {
        view! {
            <segment label={format!("{filter:?}")}
                     selected={move || todos.filter.get() == filter} on_click={move || window.navigate(filter.route())}
                     test_id={format!("filter-{filter:?}").to_lowercase()}/>
        }
    });
    view! {
        <segmented_control>{..tabs}</segmented_control>
    }
}

#[component]
fn hint() -> impl View {
    view! {
        <div flex flex_wrap items_center justify_center gap=Space::X1 mt=Space::X6>
            <text small muted>"Double-click a todo to edit it."</text>
            <kbd>"Enter"</kbd>
            <text small muted>"saves,"</text>
            <kbd>"Esc"</kbd>
            <text small muted>"cancels."</text>
        </div>
    }
}

#[derive(Clone, Copy)]
struct Done(Todos, u64);

impl CheckModel for Done {
    fn state(self) -> CheckState {
        let Done(todos, id) = self;
        match todos.items.with(&id, |todo| todo.done) {
            Some(true) => CheckState::Checked,
            _ => CheckState::Unchecked,
        }
    }

    fn toggle(self) {
        let Done(todos, id) = self;
        todos.toggle(id);
    }
}

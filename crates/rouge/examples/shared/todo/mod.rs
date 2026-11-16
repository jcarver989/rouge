mod model;
mod sky;
mod tokens;

use model::{Filter, Todos};
use rouge::prelude::*;
use rouge::widgets::{
    CheckModel, CheckState, IconName, icon_button, inline_edit, segment, segmented_control,
};
use sky::night_sky;
use tokens::{
    ACCENT, BODY_SIZE, CARD_RADIUS, CARD_WIDTH, CHECKBOX, COMPACT_WIDTH, CONTROL, DANGER_TINT,
    DUSK, EDIT_RING, EDITOR_HEIGHT, FAINT, FOOTER_Y, FROST, GAP, GLASS, HAIRLINE, HEADER_Y,
    HEADING, INSET, LINE, NIGHT, PAGE_BOTTOM, PAGE_TOP, RING, ROSE, ROW_ENTER, ROW_EXIT,
    ROW_HEIGHT, Radius, SMALL_SIZE, STACK, STAR, TAB_HEIGHT, TAB_WIDTH, TINT, TITLE_LINE_HEIGHT,
    TITLE_SIZE, TITLE_WEIGHT, UI_FONT, UNCHECKED_SCALE, WASH,
};

pub(crate) fn todo_app() -> impl View {
    Theme::provide(UiPalette {
        accent: Rgba::rgb(ACCENT),
        ..UiPalette::dark()
    });
    let todos = Todos::new();
    let has_todos = move || !todos.items.is_empty();
    view! {
        <div size_full relative bg=NIGHT font=UI_FONT text_color=FROST>
            <night_sky/>
            <scroll_view size_full>
                <scroll>
                    <div flex col items_center w_full px=INSET pt=PAGE_TOP pb=PAGE_BOTTOM>
                        <text text_size=TITLE_SIZE font_weight=TITLE_WEIGHT line_height=TITLE_LINE_HEIGHT color=HEADING>
                            "todos"
                        </text>
                        <div flex col w_full max_w=CARD_WIDTH mt=STACK rounded=CARD_RADIUS test_id="card"
                             backdrop=GLASS shadow={BoxShadow::lg()}>
                            <new_todo todos/>
                            <show when=has_todos>
                                <todo_list todos/>
                                <footer todos/>
                            </show>
                        </div>
                        <show when=has_todos>
                            <text text_size=SMALL_SIZE color=DUSK mt=STACK>"Double-click a todo to edit it"</text>
                        </show>
                    </div>
                </scroll>
            </scroll_view>
        </div>
    }
}

#[component]
fn new_todo(todos: Todos) -> impl View {
    let draft = signal(String::new());
    let add = move |title: &str| {
        todos.add(title);
        draft.set(String::new());
    };
    view! {
        <div flex items_center gap=GAP px=INSET py=HEADER_Y>
            <div flex_none size=CONTROL>
                <show when={move || !todos.items.is_empty()}>
                    <icon_button icon=IconName::ChevronDown label="Toggle all" size=CONTROL rounded=Radius::Full
                                 color={move || if todos.remaining.get() == 0 { FROST } else { FAINT }}
                                 hover:bg=TINT focus_visible:bg=TINT
                                 on_click={move || todos.toggle_all()} test_id="toggle-all"/>
                </show>
            </div>
            <div flex_1 min_w=0.0>
                <editable value=draft w_full text_size=BODY_SIZE color=FROST caret_color=STAR
                          placeholder=("What needs to be done?", DUSK)
                          autofocus on_submit=add test_id="new-todo"/>
            </div>
        </div>
    }
}

#[component]
fn todo_list(todos: Todos) -> impl View {
    view! {
        <div flex col border_t=HAIRLINE border_color=LINE>
            <each items=todos.visible key={|id| *id} let:id
                  enter=(ROW_ENTER, Spring::snappy()) exit=(ROW_EXIT, Spring::snappy())>
                <todo_row todos id={id.get_untracked()}/>
            </each>
        </div>
    }
}

#[component]
fn todo_row(todos: Todos, id: u64) -> impl View {
    let done = todos.items.select(id, |todo| todo.done);
    let title = todos.items.select(id, |todo| todo.title.clone());
    view! {
        <div flex items_center gap=GAP px=INSET h=ROW_HEIGHT border_b=HAIRLINE border_color=LINE
             hover:bg=WASH group="todo" layout_transition={Spring::smooth()} test_scope={format!("todo-{id}")}>
            <div flex flex_none items_center justify_center size=CONTROL
                 scale={done.pick(1.0, UNCHECKED_SCALE)} transition={Spring::bouncy()}>
                <checkbox checked={Done(todos, id)} mark_color=NIGHT size=CHECKBOX rounded=Radius::Full border=RING bg=Color::TRANSPARENT
                          border_color=FAINT hover:border_color=FROST focus_visible:border_color=FROST
                          checked:bg=STAR checked:border_color=STAR test_id="toggle"/>
            </div>
            <inline_edit value=title flex_1 font=UI_FONT text_size=BODY_SIZE
                         text_color={done.pick(FAINT, FROST)} strikethrough=done
                         editing:flex editing:items_center editing:h=EDITOR_HEIGHT editing:px=GAP editing:ml={-(GAP + HAIRLINE)}
                         editing:bg=WASH editing:border_color=EDIT_RING editing:rounded=Radius::Control editing:text_color=FROST
                         on_commit={move |title: String| todos.rename(id, &title)} test_id="title"/>
            <icon_button icon=IconName::X label="Delete" size=CONTROL rounded=Radius::Full color=ROSE
                         invisible group_hover=("todo", Styled::visible) hover:bg=DANGER_TINT
                         on_click={move || { todos.items.remove(&id); }} test_id="delete"/>
        </div>
    }
}

#[component]
fn footer(todos: Todos) -> impl View {
    let window = window_size();
    let compact = move || window.get().width < COMPACT_WIDTH;
    let items_left = move || match todos.remaining.get() {
        1 => "1 item left".to_owned(),
        count => format!("{count} items left"),
    };
    view! {
        <div relative flex justify_center px=INSET pt=FOOTER_Y text_size=SMALL_SIZE text_color=DUSK
             pb={move || if compact() { FOOTER_Y + GAP + TAB_HEIGHT } else { FOOTER_Y }}>
            <div absolute left=INSET right=INSET bottom=FOOTER_Y h=TAB_HEIGHT flex items_center justify_between>
                <text test_id="items-left">{items_left}</text>
                <show when={move || todos.remaining.get() < todos.items.len()}>
                    <div flex items_center h=TAB_HEIGHT px=GAP rounded=Radius::Full
                         hover:text_color=FROST focus_visible:bg=TINT
                         on_activate={move || todos.items.retain(|todo| !todo.done)} test_id="clear-completed">
                        "Clear completed"
                    </div>
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
            <segment label={format!("{filter:?}")} w=TAB_WIDTH h_full rounded=Radius::Full text_color=DUSK
                     selected={move || todos.filter.get() == filter} on_click={move || window.navigate(filter.route())}
                     test_id={format!("filter-{filter:?}").to_lowercase()}/>
        }
    });
    view! {
        <segmented_control h=TAB_HEIGHT p=0.0 gap=0.0 bg=Color::TRANSPARENT
                           indicator:rounded=Radius::Full indicator:bg=TINT indicator:border=HAIRLINE indicator:border_color=LINE>
            {..tabs}
        </segmented_control>
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

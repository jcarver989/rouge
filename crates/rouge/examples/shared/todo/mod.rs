mod model;
mod sky;
mod tokens;

use model::{Filter, Todos};
use rouge::core::{Prop, SpanStyle};
use rouge::prelude::*;
use rouge::widgets::inline_edit;
use sky::night_sky;
use std::ops::Range;
use tokens::{
    ACCENT, BODY_SIZE, CARD_RADIUS, CARD_WIDTH, CHECKBOX, CONTROL, DANGER_TINT, DUSK, EDIT_RING,
    EDITOR_HEIGHT, FAINT, FOOTER_SIDE, FOOTER_Y, FROST, GAP, GLASS, HAIRLINE, HEADER_Y, HEADING,
    ICON, ICON_CHECK, ICON_CHEVRON, ICON_CROSS, INSET, LINE, NIGHT, PAGE_BOTTOM, PAGE_TOP, RING,
    ROSE, ROW_ENTER, ROW_EXIT, ROW_HEIGHT, Radius, SMALL_SIZE, STACK, STAR, STROKE, TAB_HEIGHT,
    TAB_WIDTH, TINT, TITLE_LINE_HEIGHT, TITLE_SIZE, TITLE_WEIGHT, UI_FONT, UNCHECKED_SCALE, WASH,
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
                        <div flex col w_full max_w=CARD_WIDTH mt=STACK rounded=CARD_RADIUS
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
                    <icon_button path=ICON_CHEVRON color={move || if todos.all_done() { FROST } else { FAINT }}
                                 hover:bg=TINT focus_visible:bg=TINT
                                 on_activate={move || todos.toggle_all()} test_id="toggle-all"/>
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
    let title = todos.title(id);
    let done = todos.done(id);
    let editing = signal(false);
    view! {
        <div flex items_center gap=GAP px=INSET h=ROW_HEIGHT border_b=HAIRLINE border_color=LINE
             hover:bg=WASH group="todo" layout_transition={Spring::smooth()} test_id={format!("todo-{id}")}>
            <div flex flex_none items_center justify_center size=CONTROL>
                <check_circle checked=done on_activate={move || todos.toggle(id)}
                              test_id={format!("todo-{id}-toggle")}/>
            </div>
            <show when={move || editing.get()} otherwise={move || view! { <todo_title id title done editing/> }}>
                <todo_editor todos id title editing/>
            </show>
            <icon_button path=ICON_CROSS color=ROSE invisible group_hover=("todo", Styled::visible)
                         hover:bg=DANGER_TINT
                         on_activate={move || todos.remove(id)} test_id={format!("todo-{id}-delete")}/>
        </div>
    }
}

#[component]
fn todo_title(id: u64, title: Memo<String>, done: Memo<bool>, editing: Signal<bool>) -> impl View {
    let edit_on_double_click = move |event: &PointerEvent| {
        if event.click_count == 2 {
            editing.set(true);
        }
    };
    view! {
        <div flex_1 min_w=0.0 on_click=edit_on_double_click test_id={format!("todo-{id}-title")}>
            <text text_size=BODY_SIZE color={move || if done.get() { FAINT } else { FROST }}
                  spans={move || struck_through(done.get(), title.with(String::len))}>
                "{title}"
            </text>
        </div>
    }
}

#[component]
fn todo_editor(todos: Todos, id: u64, title: Memo<String>, editing: Signal<bool>) -> impl View {
    let draft = signal(title.get_untracked());
    let commit = move |title: String| {
        editing.set(false);
        todos.rename(id, &title);
    };
    view! {
        <inline_edit value=draft flex flex_1 items_center h=EDITOR_HEIGHT px=GAP ml={-(GAP + HAIRLINE)}
                     font=UI_FONT text_size=BODY_SIZE text_color=FROST
                     bg=WASH border_color=EDIT_RING rounded=Radius::Control
                     on_commit=commit on_cancel={move || editing.set(false)}
                     test_id={format!("todo-{id}-edit")}/>
    }
}

#[component]
fn footer(todos: Todos) -> impl View {
    let items_left = move || match todos.remaining.get() {
        1 => "1 item left".to_owned(),
        count => format!("{count} items left"),
    };
    view! {
        <div flex items_center justify_between gap=GAP px=INSET py=FOOTER_Y text_size=SMALL_SIZE text_color=DUSK>
            <div w=FOOTER_SIDE>
                <text>{items_left}</text>
            </div>
            <filter_tabs todos/>
            <div flex justify_end w=FOOTER_SIDE>
                <show when={move || todos.has_completed()}>
                    <div flex items_center h=TAB_HEIGHT px=GAP rounded=Radius::Full
                         hover:text_color=FROST focus_visible:bg=TINT
                         on_activate={move || todos.clear_completed()} test_id="clear-completed">
                        "Clear completed"
                    </div>
                </show>
            </div>
        </div>
    }
}

#[component]
fn filter_tabs(todos: Todos) -> impl View {
    let tabs = Filter::ALL.map(|filter| view! { <filter_tab todos filter/> });
    view! {
        <div relative flex h=TAB_HEIGHT>
            <div absolute top=0.0 left=0.0 w=TAB_WIDTH h_full rounded=Radius::Full
                 bg=TINT border=HAIRLINE border_color=LINE
                 translate_x={move || todos.filter.get() as usize as f32 * TAB_WIDTH}
                 transition={Spring::snappy()}/>
            {..tabs}
        </div>
    }
}

#[component]
fn filter_tab(todos: Todos, filter: Filter) -> impl View {
    view! {
        <div flex items_center justify_center w=TAB_WIDTH h_full rounded=Radius::Full
             is_selected={move || todos.filter.get() == filter} selected:text_color=FROST
             focus_visible:bg=WASH on_activate={move || todos.show(filter)}
             test_id={format!("filter-{}", filter.label().to_lowercase())}>
            <text>{filter.label()}</text>
        </div>
    }
}

#[component]
fn check_circle(checked: Memo<bool>) -> Div {
    view! {
        <div flex flex_none items_center justify_center size=CHECKBOX rounded=Radius::Full
             border=RING border_color=FAINT hover:border_color=FROST focus_visible:border_color=FROST
             is_checked=checked checked:bg=STAR checked:border_color=STAR
             scale={move || if checked.get() { 1.0 } else { UNCHECKED_SCALE }} transition={Spring::bouncy()}>
            <show when={move || checked.get()}>
                <icon path={Path::svg(ICON_CHECK)} stroke_round=STROKE size=ICON color=NIGHT/>
            </show>
        </div>
    }
}

#[component]
fn icon_button(path: &'static str, #[prop(reactive)] color: Prop<Fill>) -> Div {
    view! {
        <div flex flex_none items_center justify_center size=CONTROL rounded=Radius::Full>
            <icon path={Path::svg(path)} stroke_round=STROKE size=ICON color=color/>
        </div>
    }
}

fn struck_through(done: bool, len: usize) -> Vec<(Range<usize>, SpanStyle)> {
    let style = SpanStyle {
        strikethrough: Some(true),
        ..SpanStyle::default()
    };
    done.then_some((0..len, style)).into_iter().collect()
}

use rouge::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Todo {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) done: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Filter {
    All,
    Active,
    Completed,
}

impl Filter {
    pub(crate) const ALL: [Filter; 3] = [Filter::All, Filter::Active, Filter::Completed];

    pub(crate) fn parse(route: &str) -> Filter {
        match route {
            "/active" => Filter::Active,
            "/completed" => Filter::Completed,
            _ => Filter::All,
        }
    }

    pub(crate) fn route(self) -> &'static str {
        match self {
            Filter::All => "/",
            Filter::Active => "/active",
            Filter::Completed => "/completed",
        }
    }

    pub(crate) fn includes(self, todo: &Todo) -> bool {
        match self {
            Filter::All => true,
            Filter::Active => !todo.done,
            Filter::Completed => todo.done,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Active => "Active",
            Filter::Completed => "Completed",
        }
    }
}

pub(crate) fn tidy(title: &str) -> Option<String> {
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

#[derive(Clone, Copy)]
pub(crate) struct Todos {
    pub(crate) items: KeyedList<u64, Todo>,
    pub(crate) remaining: Memo<usize>,
    pub(crate) filter: Memo<Filter>,
    pub(crate) visible: Memo<Vec<u64>>,
    window: WindowHandle,
    next_id: Stored<u64>,
}

impl Todos {
    pub(crate) fn new() -> Self {
        let items = persisted("todos", keyed_list(|todo: &Todo| todo.id));
        let route = route();
        let filter = memo(move || route.with(|path| Filter::parse(path)));
        Todos {
            items,
            remaining: memo(move || {
                items.with_values(|todos| todos.iter().filter(|todo| !todo.done).count())
            }),
            filter,
            visible: memo(move || {
                let filter = filter.get();
                items.with_values(|todos| {
                    let visible = todos.iter().filter(|todo| filter.includes(todo));
                    visible.map(|todo| todo.id).collect()
                })
            }),
            window: window_handle(),
            next_id: stored(0),
        }
    }

    pub(crate) fn show(self, filter: Filter) {
        self.window.navigate(filter.route());
    }

    pub(crate) fn add(self, title: &str) {
        let Some(title) = tidy(title) else { return };
        let saved = self
            .items
            .with_values(|todos| todos.iter().map(|todo| todo.id + 1).max());
        let id = saved.unwrap_or(0).max(self.next_id.get());
        self.next_id.set(id + 1);
        self.items.push(Todo {
            id,
            title,
            done: false,
        });
    }

    pub(crate) fn rename(self, id: u64, title: &str) {
        match tidy(title) {
            Some(title) => {
                self.items.update(&id, |todo| todo.title = title);
            }
            None => self.remove(id),
        }
    }

    pub(crate) fn remove(self, id: u64) {
        self.items.remove(&id);
    }

    pub(crate) fn clear_completed(self) {
        self.items.retain(|todo| !todo.done);
    }

    pub(crate) fn has_completed(self) -> bool {
        self.remaining.get() < self.items.len()
    }

    pub(crate) fn title(self, id: u64) -> Memo<String> {
        self.items.select(id, |todo| todo.title.clone())
    }

    pub(crate) fn done(self, id: u64) -> Memo<bool> {
        self.items.select(id, |todo| todo.done)
    }

    pub(crate) fn toggle(self, id: u64) {
        self.items.update(&id, |todo| todo.done = !todo.done);
    }

    pub(crate) fn all_done(self) -> bool {
        !self.items.is_empty() && self.remaining.get() == 0
    }

    pub(crate) fn toggle_all(self) {
        let done = self.remaining.get_untracked() > 0;
        self.items.update_all(|todo| todo.done = done);
    }
}

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

    pub(crate) fn route(self) -> &'static str {
        match self {
            Filter::All => "/",
            Filter::Active => "/active",
            Filter::Completed => "/completed",
        }
    }

    fn includes(self, todo: &Todo) -> bool {
        match self {
            Filter::All => true,
            Filter::Active => !todo.done,
            Filter::Completed => todo.done,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Todos {
    pub(crate) items: KeyedList<u64, Todo>,
    pub(crate) remaining: Memo<usize>,
    pub(crate) filter: Memo<Filter>,
    pub(crate) visible: Memo<Vec<u64>>,
    next_id: Stored<u64>,
}

impl Todos {
    pub(crate) fn new() -> Self {
        let items = persisted("todos", keyed_list(|todo: &Todo| todo.id));
        let route = route();
        let filter = memo(move || {
            let path = route.get();
            let found = Filter::ALL
                .into_iter()
                .find(|filter| filter.route() == path);
            found.unwrap_or(Filter::All)
        });
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
            next_id: stored(0),
        }
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
            Some(title) => self.items.update(&id, |todo| todo.title = title),
            None => self.items.remove(&id).is_some(),
        };
    }

    pub(crate) fn toggle(self, id: u64) {
        self.items.update(&id, |todo| todo.done = !todo.done);
    }

    pub(crate) fn toggle_all(self) {
        let done = self.remaining.get_untracked() > 0;
        self.items.update_all(|todo| todo.done = done);
    }
}

fn tidy(title: &str) -> Option<String> {
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

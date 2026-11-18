use crate::event::{Chord, IntoChord, Key, KeyEvent, Modifiers};
use crate::path::Path;
use crate::tree::edit::EditOp;
use crate::ui::Ui;
use crate::view::Queue;
use rouge_reactive::{ReadSignal, Signal, expect_context};
pub use rouge_theme::Appearance;
use std::any::Any;
use std::rc::Rc;
use std::slice::from_ref;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decorations {
    #[default]
    Server,
    Client,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[derive(Clone)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Menu {
    pub fn edit() -> Self {
        let mut items: Vec<_> = EDIT_COMMANDS
            .iter()
            .map(|&(action, title, key, shift)| {
                let modifiers = Modifiers {
                    shift,
                    ..Modifiers::COMMAND
                };
                MenuItem::action(title, action).shortcut(Chord::new(Key::character(key), modifiers))
            })
            .collect();
        items.insert(2, MenuItem::Separator);
        Menu::new("Edit", items)
    }

    pub fn app(name: &str, mut items: Vec<MenuItem>) -> Self {
        if !items.is_empty() {
            items.push(MenuItem::Separator);
        }
        items.push(
            MenuItem::callback(format!("Quit {name}"), || window_handle().quit())
                .shortcut(Chord::new(Key::character("q"), Modifiers::COMMAND)),
        );
        Menu::new(name, items)
    }

    pub fn window() -> Self {
        Menu::new(
            "Window",
            vec![
                MenuItem::callback("Minimize", || window_handle().minimize()),
                MenuItem::callback("Zoom", || window_handle().zoom()),
            ],
        )
    }

    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Menu {
            title: title.into(),
            items,
        }
    }
}

#[derive(Clone)]
pub enum MenuAction {
    Typed(Rc<dyn Any>),
    Callback(Rc<dyn Fn()>),
}

#[derive(Clone)]
pub enum MenuItem {
    Item {
        title: String,
        action: MenuAction,
        shortcut: Option<Chord>,
        id: Option<String>,
        icon: Option<Path>,
        destructive: bool,
    },
    Separator,
    Label(String),
    Submenu(Menu),
}

impl MenuItem {
    pub fn action<T: Any>(title: impl Into<String>, action: T) -> Self {
        MenuItem::new(title, MenuAction::Typed(Rc::new(action)))
    }

    pub fn callback(title: impl Into<String>, on_select: impl Fn() + 'static) -> Self {
        MenuItem::new(title, MenuAction::Callback(Rc::new(on_select)))
    }

    fn new(title: impl Into<String>, action: MenuAction) -> Self {
        MenuItem::Item {
            title: title.into(),
            action,
            shortcut: None,
            id: None,
            icon: None,
            destructive: false,
        }
    }

    pub fn label(title: impl Into<String>) -> Self {
        MenuItem::Label(title.into())
    }

    pub fn activate(&self) {
        match self {
            MenuItem::Item {
                action: MenuAction::Callback(on_select),
                ..
            } => on_select(),
            MenuItem::Item { .. } => window_handle().dispatch_menu_item(self.clone()),
            MenuItem::Separator | MenuItem::Label(_) | MenuItem::Submenu(_) => {}
        }
    }

    #[track_caller]
    pub fn shortcut(mut self, chord: impl IntoChord) -> Self {
        if let MenuItem::Item { shortcut, .. } = &mut self {
            *shortcut = Some(chord.into_chord());
        }
        self
    }

    pub fn id(mut self, value: impl Into<String>) -> Self {
        if let MenuItem::Item { id, .. } = &mut self {
            *id = Some(value.into());
        }
        self
    }

    pub fn icon(mut self, value: impl Into<Path>) -> Self {
        if let MenuItem::Item { icon, .. } = &mut self {
            *icon = Some(value.into());
        }
        self
    }

    pub fn destructive(mut self) -> Self {
        if let MenuItem::Item { destructive, .. } = &mut self {
            *destructive = true;
        }
        self
    }

    pub fn shortcuts(menus: &[Menu]) -> Vec<(Chord, MenuItem)> {
        menus
            .iter()
            .flat_map(|menu| &menu.items)
            .flat_map(|item| match item {
                MenuItem::Item {
                    shortcut: Some(chord),
                    ..
                } => vec![(chord.clone(), item.clone())],
                MenuItem::Submenu(menu) => MenuItem::shortcuts(from_ref(menu)),
                _ => Vec::new(),
            })
            .collect()
    }

    pub fn dispatch(&self, ui: &mut Ui) -> bool {
        match self {
            MenuItem::Item {
                action: MenuAction::Typed(action),
                ..
            } => ui.dispatch_action_ref(&**action),
            MenuItem::Item {
                action: MenuAction::Callback(on_select),
                ..
            } => {
                ui.invoke(|| on_select());
                true
            }
            MenuItem::Separator | MenuItem::Label(_) | MenuItem::Submenu(_) => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

const EDIT_COMMANDS: [(EditAction, &str, &str, bool); 6] = [
    (EditAction::Undo, "Undo", "z", false),
    (EditAction::Redo, "Redo", "z", true),
    (EditAction::Cut, "Cut", "x", false),
    (EditAction::Copy, "Copy", "c", false),
    (EditAction::Paste, "Paste", "v", false),
    (EditAction::SelectAll, "Select All", "a", false),
];

impl EditAction {
    pub(crate) fn from_key(event: &KeyEvent) -> Option<Self> {
        let Key::Character(key) = &event.key else {
            return None;
        };
        if !event.modifiers.command() {
            return None;
        }
        if key == "y" {
            return Some(Self::Redo);
        }
        let shift = event.modifiers.shift;
        EDIT_COMMANDS
            .iter()
            .find(|(_, _, k, s)| k == key && (*k != "z" || *s == shift))
            .map(|command| command.0)
    }

    pub(crate) fn operation(self) -> EditOp {
        match self {
            Self::Undo => EditOp::Undo,
            Self::Redo => EditOp::Redo,
            Self::Cut => EditOp::Cut,
            Self::Copy => EditOp::Copy,
            Self::Paste => EditOp::Paste,
            Self::SelectAll => EditOp::SelectAll,
        }
    }
}

pub enum WindowCommand {
    SetTitle(String),
    Navigate(String),
    Minimize,
    Zoom,
    Close,
    StartDrag,
    StartResize(ResizeEdge),
    Quit,
    Focus,
    Menu(MenuItem),
}

pub(crate) enum Sent {
    Window(WindowCommand),
    Copy(String),
}

#[derive(Clone, Copy)]
pub struct WindowHandle {
    pub(crate) queue: Signal<Queue>,
    pub(crate) title: Signal<String>,
    pub(crate) route: Signal<String>,
    pub(crate) appearance: Signal<Appearance>,
    pub(crate) focused: Signal<bool>,
}

pub fn window_handle() -> WindowHandle {
    expect_context::<WindowHandle>()
}

pub fn appearance() -> ReadSignal<Appearance> {
    window_handle().appearance.read_only()
}

pub fn window_focused() -> ReadSignal<bool> {
    window_handle().focused.read_only()
}

pub fn route() -> ReadSignal<String> {
    window_handle().route.read_only()
}

impl WindowHandle {
    fn send(self, command: WindowCommand) {
        self.post(Sent::Window(command));
    }

    fn post(self, command: Sent) {
        if self.queue.is_alive() {
            self.queue.with_untracked(|queue| queue.post(command));
        }
    }

    pub fn title(self) -> ReadSignal<String> {
        self.title.read_only()
    }

    pub fn set_title(self, title: impl Into<String>) {
        let title = title.into();
        if !self.title.is_alive() {
            return;
        }
        self.title.set(title.clone());
        self.send(WindowCommand::SetTitle(title));
    }

    pub fn navigate(self, path: impl Into<String>) {
        let path = path.into();
        if self.route.is_alive() && self.route.set_if_changed(path.clone()) {
            self.send(WindowCommand::Navigate(path));
        }
    }

    pub fn minimize(self) {
        self.send(WindowCommand::Minimize);
    }
    pub fn zoom(self) {
        self.send(WindowCommand::Zoom);
    }
    pub fn close(self) {
        self.send(WindowCommand::Close);
    }
    pub fn start_drag(self) {
        self.send(WindowCommand::StartDrag);
    }
    pub fn start_resize(self, edge: ResizeEdge) {
        self.send(WindowCommand::StartResize(edge));
    }
    pub fn quit(self) {
        self.send(WindowCommand::Quit);
    }

    pub fn focus(self) {
        self.send(WindowCommand::Focus);
    }

    pub fn dispatch_action<T: Any>(self, action: T) {
        self.dispatch_menu_item(MenuItem::action(String::new(), action));
    }

    pub fn dispatch_menu_item(self, item: MenuItem) {
        self.send(WindowCommand::Menu(item));
    }

    pub fn copy_text(self, text: impl Into<String>) {
        self.post(Sent::Copy(text.into()));
    }
}

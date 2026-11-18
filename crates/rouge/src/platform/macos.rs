use super::Platform;
use super::native::clipboard::DesktopClipboard;
use super::native::{WinitPlatform, run};
use crate::app::{App, AppError};
use muda::accelerator::{Key as NativeKey, KeyAccelerator, Modifiers};
use muda::{
    Menu as NativeMenu, MenuEvent, MenuId, MenuItem as NativeItem, PredefinedMenuItem, Submenu,
};
use rouge_core::{Chord, Clipboard, Key, Menu, MenuItem, Ui};
use std::collections::HashMap;
#[cfg(feature = "persist")]
use std::env::home_dir;
#[cfg(feature = "open-url")]
use std::io::Error as IoError;
#[cfg(feature = "persist")]
use std::path::PathBuf;
use thiserror::Error;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use winit::event::KeyEvent;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::Key as WinitKey;
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to create application menu: {0}")]
    Menu(#[from] muda::Error),
}

#[derive(Default)]
pub(crate) struct Options;

pub(crate) struct MacOs {
    menus: NativeMenus,
}

impl Platform for MacOs {
    type Options = Options;

    const MENU_BAR: bool = false;
    const WINDOW_MENU: bool = true;

    fn run(app: App, options: Self::Options) -> Result<(), AppError> {
        run(app, options)
    }

    #[cfg(feature = "open-url")]
    fn open_url(url: &str) -> Result<(), IoError> {
        open::that(url)
    }
}

impl WinitPlatform for MacOs {
    fn new(menus: &[Menu], waker: &EventLoopProxy<()>) -> Result<Self, AppError> {
        let menus = NativeMenus::new(menus.to_vec(), waker.clone()).map_err(Error::Menu)?;
        Ok(MacOs { menus })
    }

    fn clipboard() -> impl Clipboard + 'static {
        DesktopClipboard::new()
    }

    #[cfg(feature = "persist")]
    fn default_config_dir(name: &str) -> Option<PathBuf> {
        home_dir().map(|home| home.join("Library/Application Support").join(name))
    }

    fn key(event: &KeyEvent) -> WinitKey {
        event.key_without_modifiers()
    }

    fn menu_events(&mut self, ui: Option<&mut Ui>) -> bool {
        self.menus.dispatch(ui)
    }
}

struct NativeMenus {
    _menu: NativeMenu,
    actions: HashMap<MenuId, MenuItem>,
    quit: MenuId,
    events: UnboundedReceiver<MenuId>,
}

impl NativeMenus {
    fn new(menus: Vec<Menu>, proxy: EventLoopProxy<()>) -> Result<Self, muda::Error> {
        let menu = NativeMenu::new();
        let app = Submenu::new("Application", true);
        app.append(&PredefinedMenuItem::about(None, None))?;
        app.append(&PredefinedMenuItem::separator())?;
        app.append(&PredefinedMenuItem::hide(None))?;
        app.append(&PredefinedMenuItem::hide_others(None))?;
        app.append(&PredefinedMenuItem::show_all(None))?;
        app.append(&PredefinedMenuItem::separator())?;
        let quit = NativeItem::new(
            "Quit",
            true,
            Some("Super+Q".parse().expect("valid shortcut")),
        );
        app.append(&quit)?;
        menu.append(&app)?;
        let mut actions = HashMap::new();
        for submenu in menus {
            menu.append(&build_menu(submenu, &mut actions)?)?;
        }
        menu.init_for_nsapp();
        let (sender, events) = unbounded_channel();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = sender.send(event.id);
            let _ = proxy.send_event(());
        }));
        Ok(Self {
            _menu: menu,
            actions,
            quit: quit.id().clone(),
            events,
        })
    }

    fn dispatch(&mut self, mut ui: Option<&mut Ui>) -> bool {
        let mut quit = false;
        while let Ok(id) = self.events.try_recv() {
            if id == self.quit {
                quit = true;
            }
            if let (Some(item), Some(ui)) = (self.actions.get(&id), ui.as_deref_mut()) {
                item.dispatch(ui);
            }
        }
        quit
    }
}

fn build_menu(menu: Menu, actions: &mut HashMap<MenuId, MenuItem>) -> Result<Submenu, muda::Error> {
    let native = Submenu::new(menu.title, true);
    for item in menu.items {
        match &item {
            MenuItem::Separator => native.append(&PredefinedMenuItem::separator())?,
            MenuItem::Label(title) => native.append(&NativeItem::new(title, false, None))?,
            MenuItem::Submenu(menu) => native.append(&build_menu(menu.clone(), actions)?)?,
            MenuItem::Item {
                title, shortcut, ..
            } => {
                let action = NativeItem::new(title, true, None);
                action.set_key_accelerator(shortcut.as_ref().and_then(accelerator))?;
                native.append(&action)?;
                actions.insert(action.id().clone(), item);
            }
        }
    }
    Ok(native)
}

fn accelerator(chord: &Chord) -> Option<KeyAccelerator> {
    let mut modifiers = Modifiers::empty();
    if chord.modifiers.ctrl {
        modifiers |= Modifiers::CONTROL;
    }
    if chord.modifiers.meta {
        modifiers |= Modifiers::SUPER;
    }
    if chord.modifiers.alt {
        modifiers |= Modifiers::ALT;
    }
    if chord.modifiers.shift {
        modifiers |= Modifiers::SHIFT;
    }
    let key = match &chord.key {
        Key::Character(text) => NativeKey::Character(text.clone()),
        Key::Space => NativeKey::Character(" ".into()),
        Key::Other => return None,
        key => {
            let label = match key {
                Key::F(n) => format!("F{n}"),
                key => format!("{key:?}"),
            };
            label.parse::<KeyAccelerator>().ok()?.key().clone()
        }
    };
    Some(KeyAccelerator::new(Some(modifiers), key))
}

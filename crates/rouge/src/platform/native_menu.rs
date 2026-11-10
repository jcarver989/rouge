use muda::accelerator::{Key as NativeKey, KeyAccelerator, Modifiers};
use muda::{
    Menu as NativeMenu, MenuEvent, MenuId, MenuItem as NativeItem, PredefinedMenuItem, Submenu,
};
use rouge_core::{Chord, Key, Menu, MenuItem, Ui};
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use winit::event_loop::EventLoopProxy;

pub(crate) struct NativeMenus {
    _menu: NativeMenu,
    actions: HashMap<MenuId, MenuItem>,
    quit: MenuId,
    events: UnboundedReceiver<MenuId>,
}

impl NativeMenus {
    pub(crate) fn new(menus: Vec<Menu>, proxy: EventLoopProxy<()>) -> Result<Self, muda::Error> {
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

    pub(crate) fn dispatch(&mut self, mut ui: Option<&mut Ui>) -> bool {
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
            MenuItem::Submenu(menu) => native.append(&build_menu(menu.clone(), actions)?)?,
            MenuItem::Action {
                title, shortcut, ..
            }
            | MenuItem::Callback {
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

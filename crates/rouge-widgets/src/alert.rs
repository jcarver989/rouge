use crate::components::{Button, RowAction, button, muted_text};
use crate::material::{Material, MaterialExt};
use crate::popover::{FADED, floating};
use crate::tokens::Space;
use crate::typography::UiStyles;
use crate::util::TestId;
use rouge_core::{
    Apply, BackdropBlur, BuildCx, Key, PointerEvent, Role, Styled, View, component, div, pct, text,
    view,
};
use rouge_reactive::Signal;
use rouge_theme::ButtonVariant;
use std::rc::Rc;

pub struct Alert {
    open: Signal<bool>,
    title: String,
    message: Option<String>,
    actions: Vec<RowAction>,
    cancel: Option<String>,
    test_id: Option<String>,
}

#[component]
pub fn alert(open: Signal<bool>, #[prop(into)] title: String) -> Alert {
    Alert {
        open,
        title,
        message: None,
        actions: Vec::new(),
        cancel: None,
        test_id: None,
    }
}

impl Alert {
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn action(mut self, action: RowAction) -> Self {
        self.actions.push(action);
        self
    }

    pub fn cancel(mut self, cancel: impl Into<String>) -> Self {
        self.cancel = Some(cancel.into());
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }
}

impl View for Alert {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (open, id) = (self.open, self.test_id);
        let (title, message, cancel) = (self.title, self.message, self.cancel);
        let actions: Rc<[RowAction]> = self.actions.into();
        let escapes = cancel.is_some();
        let panel = move || {
            let leave = cancel.clone().map(|label| {
                button(label)
                    .variant(ButtonVariant::Secondary)
                    .on_click(move || open.set(false))
                    .when_some(id.part("cancel"), Button::test_id)
            });
            let picks = actions.iter().cloned().enumerate().map(|(index, action)| {
                let on_select = action.on_select;
                let variant = if action.destructive {
                    ButtonVariant::Destructive
                } else {
                    ButtonVariant::Primary
                };
                button(action.label)
                    .variant(variant)
                    .on_click(move || {
                        open.set(false);
                        on_select.call();
                    })
                    .when_some(id.part(format_args!("action-{index}")), Button::test_id)
            });
            let stacked = actions.len() + usize::from(escapes) > 2;
            let mut buttons: Vec<Button> = leave.into_iter().chain(picks).collect();
            if stacked {
                buttons.rotate_left(usize::from(escapes));
            }
            let buttons = buttons
                .into_iter()
                .map(|button| view! { <div flex col flex_none min_w=0.0 when:flex_1={!stacked}>{button}</div> });
            let panel = view! {
                <div test_id?={id.clone()} flex col gap=Space::X1 p=Space::X4 w=WIDTH max_w=pct(100.0)
                     material=Material::Glass>
                    <text heading primary>{title.clone()}</text>
                    {message.clone().map(|message| view! { <muted_text>{message}</muted_text> })}
                    <div flex gap=Space::X2 pt=Space::X3 when:col=stacked>{..buttons}</div>
                </div>
            };
            view! {
                <div test_id?=id.part("scrim") absolute inset=0.0 flex items_center justify_center p=Space::X4
                     bg=Role::Scrim backdrop=BackdropBlur::new(SCRIM_BLUR) focus_scope autofocus tab_index=-1
                     on_pointer_down=PointerEvent::stop_propagation
                     on_key_down={move |event| {
                         if event.key == Key::Escape && escapes {
                             open.set(false);
                         }
                         if event.key != Key::Tab {
                             event.stop_propagation();
                         }
                     }}>
                    {panel}
                </div>
            }
        };
        floating(move || open.get(), FADED, panel).build(cx);
    }
}

const WIDTH: f32 = 272.0;
const SCRIM_BLUR: f32 = 8.0;

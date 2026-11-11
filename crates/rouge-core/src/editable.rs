use crate::event::{Cursor, ImeEvent, Key, KeyEvent, PointerButton};
use crate::focus::FocusHandle;
use crate::length::Length;
use crate::prop::IntoProp;
use crate::tree::ElementId;
use crate::tree::edit::{EditOp, EditState, TextHandler};
use crate::view::{BuildCx, Text, View, send, text};
use crate::window::EditAction;
use crate::{Styled, styled};
use rouge_macros::component;
use rouge_reactive::{Signal, effect};
use rouge_render::Color;
use rouge_text::Movement;
use std::borrow::Cow;
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

pub struct Editable {
    text: Text,
    value: Signal<String>,
    state: EditState,
    on_submit: Option<TextHandler>,
    submit_on_enter: bool,
    select_all_on_focus: bool,
}

#[component]
pub fn editable(value: Signal<String>) -> Editable {
    Editable {
        text: text(value),
        value,
        state: EditState::default(),
        on_submit: None,
        submit_on_enter: false,
        select_all_on_focus: false,
    }
}

impl Editable {
    pub fn test_id(mut self, name: impl Into<String>) -> Self {
        self.text.0 = self.text.0.test_id(name);
        self
    }
    pub fn autofocus(mut self) -> Self {
        self.text.0 = self.text.0.autofocus();
        self
    }
    pub fn selection(mut self, selection: Signal<Range<usize>>) -> Self {
        self.state.selection_signal = Some(selection);
        self
    }
    pub fn min_rows(mut self, rows: usize) -> Self {
        self.state.min_rows = rows.max(1);
        self
    }
    pub fn max_rows(mut self, rows: usize) -> Self {
        self.state.max_rows = rows.max(1);
        self
    }
    pub fn submit_on_enter(mut self, submit: bool) -> Self {
        self.submit_on_enter = submit;
        self
    }

    pub fn select_all_on_focus(mut self) -> Self {
        self.select_all_on_focus = true;
        self
    }

    pub fn multiline(mut self) -> Self {
        self.state.multiline = true;
        self
    }

    pub fn on_submit(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    pub fn selection_color(mut self, color: Color) -> Self {
        self.state.selection_color = color;
        self
    }

    pub fn caret_color(mut self, color: Color) -> Self {
        self.state.caret_color = Some(color);
        self
    }

    pub fn track_focus(mut self, handle: FocusHandle) -> Self {
        self.text.0 = self.text.0.track_focus(handle);
        self
    }

    pub fn text_size<U>(mut self, size: impl IntoProp<Length, U>) -> Self {
        self.text = self.text.text_size(size);
        self
    }

    pub fn color<U>(mut self, color: impl IntoProp<Color, U>) -> Self {
        self.text = self.text.color(color);
        self
    }

    pub fn font(mut self, family: impl Into<Cow<'static, str>>) -> Self {
        self.text = self.text.font(family.into().into_owned());
        self
    }

    pub fn line_height(mut self, multiple: f32) -> Self {
        self.text = self.text.line_height(multiple);
        self
    }
}

styled!(Editable => text);

fn key_op(event: &KeyEvent, multiline: bool) -> Option<EditOp> {
    let modifiers = event.modifiers;
    let (extend, word) = (modifiers.shift, modifiers.word());
    let line = cfg!(target_os = "macos") && modifiers.meta;
    let horizontal = |by_word: Movement, by_line: Movement, by_char: Movement| {
        let movement = if word {
            by_word
        } else if line {
            by_line
        } else {
            by_char
        };
        EditOp::Move(movement, extend)
    };
    if let Some(action) = EditAction::from_key(event) {
        return Some(action.operation());
    }
    Some(match &event.key {
        Key::ArrowLeft => horizontal(Movement::WordLeft, Movement::LineStart, Movement::Left),
        Key::ArrowRight => horizontal(Movement::WordRight, Movement::LineEnd, Movement::Right),
        Key::ArrowUp if line => EditOp::Move(Movement::Start, extend),
        Key::ArrowDown if line => EditOp::Move(Movement::End, extend),
        Key::ArrowUp if multiline => EditOp::Move(Movement::Up, extend),
        Key::ArrowDown if multiline => EditOp::Move(Movement::Down, extend),
        Key::Home if modifiers.ctrl => EditOp::Move(Movement::Start, extend),
        Key::End if modifiers.ctrl => EditOp::Move(Movement::End, extend),
        Key::Home => EditOp::Move(Movement::LineStart, extend),
        Key::End => EditOp::Move(Movement::LineEnd, extend),
        Key::Backspace if word => EditOp::Erase(Movement::WordLeft),
        Key::Backspace => EditOp::Erase(Movement::Left),
        Key::Delete if word => EditOp::Erase(Movement::WordRight),
        Key::Delete => EditOp::Erase(Movement::Right),
        Key::Enter if multiline => EditOp::Insert("\n".into()),
        _ => {
            let typed = event
                .text
                .as_ref()
                .filter(|_| !(modifiers.ctrl || modifiers.meta))?;
            let printable = !typed.is_empty() && !typed.chars().any(char::is_control);
            printable.then(|| EditOp::Insert(typed.clone()))?
        }
    })
}

impl View for Editable {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let Editable {
            text: Text(mut div),
            value,
            mut state,
            on_submit,
            submit_on_enter,
            select_all_on_focus,
        } = self;
        let multiline = state.multiline;
        let selection = state.selection_signal;
        let target = Rc::new(Cell::new(None));
        let (element, patches) = (target.clone(), cx.patches.clone());
        let edit: Rc<dyn Fn(EditOp)> = Rc::new(move |op| {
            if let Some(id) = element.get() {
                send(&patches, move |cx| cx.tree.edit(id, op, cx.text));
            }
        });
        state.on_change = Some(Rc::new(move |content: &str| {
            if value.is_alive() {
                value.set_if_changed(content.to_owned());
            }
        }));
        let data = div.spec.text.get_or_insert_default();
        data.content = value.get_untracked();
        data.edit = Some(state);
        data.style.wrap = multiline;

        let select = edit.clone();
        let (keys, ime, down, moved, actions) =
            (edit.clone(), edit.clone(), edit.clone(), edit.clone(), edit);
        let dragging = Rc::new(Cell::new(false));
        let (pressed, released) = (dragging.clone(), dragging.clone());
        let id = div
            .focusable()
            .on_action(move |action: &EditAction| {
                actions(action.operation());
            })
            .on_focus(move |_| {
                if select_all_on_focus {
                    select(EditOp::SelectAll);
                }
            })
            .cursor(Cursor::Text)
            .on_key_down(move |event| {
                if event.key == Key::Enter
                    && !event.modifiers.shift
                    && (submit_on_enter || !multiline)
                {
                    if let Some(on_submit) = &on_submit {
                        on_submit(&value.get_untracked());
                        event.stop_propagation();
                    }
                } else if let Some(op) = key_op(event, multiline) {
                    keys(op);
                    event.stop_propagation();
                } else if let (Key::Enter, Some(on_submit)) = (&event.key, &on_submit) {
                    on_submit(&value.get_untracked());
                    event.stop_propagation();
                }
            })
            .on_ime(move |event| {
                ime(match event.clone() {
                    ImeEvent::Preedit { text, cursor } => EditOp::Compose { text, cursor },
                    ImeEvent::Commit(text) => EditOp::Insert(text),
                });
            })
            .on_pointer_down(move |event| {
                if event.button == PointerButton::Primary {
                    event.capture();
                    pressed.set(true);
                    down(EditOp::Press {
                        local: event.local(),
                        clicks: event.click_count,
                        extend: event.modifiers.shift,
                    });
                }
            })
            .on_pointer_move(move |event| {
                if dragging.get() {
                    moved(EditOp::Drag {
                        local: event.local(),
                    });
                }
            })
            .on_pointer_up(move |_| released.set(false))
            .build(cx);
        target.set(Some(id));
        if let Some(selection) = selection {
            let patches = cx.patches.clone();
            effect(move || {
                let range = selection.get();
                send(&patches, move |cx| {
                    cx.tree.edit(id, EditOp::Select(range), cx.text);
                });
            });
        }
        id
    }
}

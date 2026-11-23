use crate::changes::TextInput as TextInputChange;
use crate::driver::Driver;
use crate::platform::editing::{Editing, Previous, selected};
use crate::platform::native::{Input, UserEvent, WinitInput};
use crate::platform::warn;
use objc2::rc::{Retained, Weak as WeakObject};
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{
    NSArray, NSAttributedStringKey, NSComparisonResult, NSDictionary, NSInteger, NSRange, NSString,
};
use objc2_ui_kit::{
    NSWritingDirection, UIKeyInput, UIKeyboardType, UIResponder, UIReturnKeyType,
    UITextAutocapitalizationType, UITextAutocorrectionType, UITextInput, UITextInputDelegate,
    UITextInputStringTokenizer, UITextInputTokenizer, UITextInputTraits, UITextLayoutDirection,
    UITextPosition, UITextRange, UITextSelectionRect, UITextStorageDirection, UIView,
};
use rouge_core::text::{Movement, byte_offset, utf16_offset};
use rouge_core::{
    ContentType, EditTransaction, Editor, EnterKey, InputHints, Key, KeyEvent, Point, Rect, Ui,
};
use std::cell::{Cell, OnceCell, RefCell};
use std::ops::Range;
use std::rc::{Rc, Weak};
use winit::event_loop::EventLoopProxy;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

struct Ivars {
    driver: Weak<RefCell<Driver>>,
    wake: Box<dyn Fn()>,
    delegate: RefCell<Option<WeakObject<ProtocolObject<dyn UITextInputDelegate>>>>,
    tokenizer: OnceCell<Retained<UITextInputStringTokenizer>>,
    known: RefCell<Option<Editing>>,
    scale: Cell<f64>,
}

define_class!(
    #[unsafe(super(UITextPosition))]
    #[thread_kind = MainThreadOnly]
    #[name = "RougeTextPosition"]
    #[ivars = usize]
    struct Position;
);

impl Position {
    fn at(mtm: MainThreadMarker, offset: usize) -> Retained<UITextPosition> {
        let this = Self::alloc(mtm).set_ivars(offset);
        let position: Retained<Self> = unsafe { msg_send![super(this), init] };
        Retained::into_super(position)
    }
}

define_class!(
    #[unsafe(super(UITextRange))]
    #[thread_kind = MainThreadOnly]
    #[name = "RougeTextRange"]
    #[ivars = Range<usize>]
    struct Span;

    impl Span {
        #[unsafe(method_id(start))]
        fn start(&self) -> Retained<UITextPosition> {
            Position::at(self.mtm(), self.ivars().start)
        }

        #[unsafe(method_id(end))]
        fn end(&self) -> Retained<UITextPosition> {
            Position::at(self.mtm(), self.ivars().end)
        }

        #[unsafe(method(isEmpty))]
        fn is_empty(&self) -> bool {
            self.ivars().is_empty()
        }
    }
);

impl Span {
    fn over(mtm: MainThreadMarker, range: Range<usize>) -> Retained<UITextRange> {
        let this =
            Self::alloc(mtm).set_ivars(range.start.min(range.end)..range.end.max(range.start));
        let span: Retained<Self> = unsafe { msg_send![super(this), init] };
        Retained::into_super(span)
    }
}

fn offset(position: &UITextPosition) -> usize {
    position
        .downcast_ref::<Position>()
        .map_or(0, |position| *position.ivars())
}

fn bytes(range: &UITextRange) -> Range<usize> {
    range
        .downcast_ref::<Span>()
        .map_or(0..0, |span| span.ivars().clone())
}

fn forward(direction: UITextLayoutDirection) -> bool {
    direction == UITextLayoutDirection::Right || direction == UITextLayoutDirection::Down
}

fn movement(direction: UITextLayoutDirection, by: NSInteger) -> Movement {
    let horizontal =
        direction == UITextLayoutDirection::Left || direction == UITextLayoutDirection::Right;
    match (horizontal, forward(direction) == (by >= 0)) {
        (true, true) => Movement::Right,
        (true, false) => Movement::Left,
        (false, true) => Movement::Down,
        (false, false) => Movement::Up,
    }
}

define_class!(
    #[unsafe(super(UIView, UIResponder, objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RougeTextInput"]
    #[ivars = Ivars]
    struct Responder;

    impl Responder {
        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            true
        }
    }

    unsafe impl NSObjectProtocol for Responder {}

    unsafe impl UITextInputTraits for Responder {
        #[unsafe(method(keyboardType))]
        fn keyboard_type(&self) -> UIKeyboardType {
            match self.hints().content {
                ContentType::Text | ContentType::Search => UIKeyboardType::Default,
                ContentType::Email => UIKeyboardType::EmailAddress,
                ContentType::Url => UIKeyboardType::URL,
                ContentType::Number => UIKeyboardType::DecimalPad,
                ContentType::Phone => UIKeyboardType::PhonePad,
            }
        }

        #[unsafe(method(returnKeyType))]
        fn return_key_type(&self) -> UIReturnKeyType {
            match self.hints().enter {
                EnterKey::Default => UIReturnKeyType::Default,
                EnterKey::Done => UIReturnKeyType::Done,
                EnterKey::Go => UIReturnKeyType::Go,
                EnterKey::Next => UIReturnKeyType::Next,
                EnterKey::Search => UIReturnKeyType::Search,
                EnterKey::Send => UIReturnKeyType::Send,
            }
        }

        #[unsafe(method(autocapitalizationType))]
        fn autocapitalization_type(&self) -> UITextAutocapitalizationType {
            let hints = self.hints();
            if hints.autocorrect && hints.content == ContentType::Text {
                UITextAutocapitalizationType::Sentences
            } else {
                UITextAutocapitalizationType::None
            }
        }

        #[unsafe(method(autocorrectionType))]
        fn autocorrection_type(&self) -> UITextAutocorrectionType {
            if self.hints().autocorrect {
                UITextAutocorrectionType::Default
            } else {
                UITextAutocorrectionType::No
            }
        }
    }

    unsafe impl UIKeyInput for Responder {
        #[unsafe(method(hasText))]
        fn has_text(&self) -> bool {
            self.query(|editor| Some(!editor.text().is_empty()))
                .unwrap_or(false)
        }

        #[unsafe(method(insertText:))]
        fn insert_text(&self, text: &NSString) {
            let text = text.to_string();
            if text == "\n" {
                return self.press(Key::Enter);
            }
            self.edit(|editor| {
                let range = editor.marked().unwrap_or_else(|| selected(editor));
                EditTransaction::new().replace(range, text)
            });
        }

        #[unsafe(method(deleteBackward))]
        fn delete_backward(&self) {
            self.press(Key::Backspace);
        }
    }

    unsafe impl UITextInput for Responder {
        #[unsafe(method_id(textInRange:))]
        fn text_in_range(&self, range: &UITextRange) -> Option<Retained<NSString>> {
            self.query(|editor| Some(NSString::from_str(editor.slice(bytes(range)))))
        }

        #[unsafe(method(replaceRange:withText:))]
        fn replace_range(&self, range: &UITextRange, text: &NSString) {
            let (range, text) = (bytes(range), text.to_string());
            self.edit(|_| EditTransaction::new().replace(range, text));
        }

        #[unsafe(method_id(selectedTextRange))]
        fn selected_text_range(&self) -> Option<Retained<UITextRange>> {
            let mtm = self.mtm();
            self.query(|editor| Some(Span::over(mtm, selected(editor))))
        }

        #[unsafe(method(setSelectedTextRange:))]
        fn set_selected_text_range(&self, range: Option<&UITextRange>) {
            let Some(range) = range.map(bytes) else {
                return;
            };
            self.edit(|editor| {
                let transaction = EditTransaction::new().select(range.start, range.end);
                match editor.marked() {
                    Some(marked) => transaction.mark(marked),
                    None => transaction,
                }
            });
        }

        #[unsafe(method_id(markedTextRange))]
        fn marked_text_range(&self) -> Option<Retained<UITextRange>> {
            let mtm = self.mtm();
            self.query(|editor| Some(Span::over(mtm, editor.marked()?)))
        }

        #[unsafe(method_id(markedTextStyle))]
        fn marked_text_style(
            &self,
        ) -> Option<Retained<NSDictionary<NSAttributedStringKey, AnyObject>>> {
            None
        }

        #[unsafe(method(setMarkedTextStyle:))]
        fn set_marked_text_style(
            &self,
            _style: Option<&NSDictionary<NSAttributedStringKey, AnyObject>>,
        ) {
        }

        #[unsafe(method(setMarkedText:selectedRange:))]
        fn set_marked_text(&self, text: Option<&NSString>, selected_range: NSRange) {
            let text = text.map_or_else(String::new, NSString::to_string);
            self.edit(|editor| {
                let range = editor.marked().unwrap_or_else(|| selected(editor));
                let start = range.start;
                let caret = |units: usize| start + byte_offset(&text, units);
                let (anchor, focus) = (
                    caret(selected_range.location),
                    caret(selected_range.location + selected_range.length),
                );
                let marked = start..start + text.len();
                let transaction = EditTransaction::new()
                    .replace(range, text)
                    .select(anchor, focus);
                if marked.is_empty() {
                    transaction
                } else {
                    transaction.mark(marked)
                }
            });
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            self.edit(|editor| {
                let (anchor, focus) = editor.selection();
                EditTransaction::new().select(anchor, focus)
            });
        }

        #[unsafe(method_id(beginningOfDocument))]
        fn beginning_of_document(&self) -> Retained<UITextPosition> {
            Position::at(self.mtm(), 0)
        }

        #[unsafe(method_id(endOfDocument))]
        fn end_of_document(&self) -> Retained<UITextPosition> {
            Position::at(self.mtm(), self.length())
        }

        #[unsafe(method_id(textRangeFromPosition:toPosition:))]
        fn text_range(
            &self,
            from: &UITextPosition,
            to: &UITextPosition,
        ) -> Option<Retained<UITextRange>> {
            Some(Span::over(self.mtm(), offset(from)..offset(to)))
        }

        #[unsafe(method_id(positionFromPosition:offset:))]
        fn position_from(
            &self,
            position: &UITextPosition,
            by: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            let (from, mtm) = (offset(position), self.mtm());
            self.query(|editor| {
                let text = editor.text();
                let target = utf16_offset(text, from).checked_add_signed(by)?;
                if target > utf16_offset(text, text.len()) {
                    return None;
                }
                let floor = byte_offset(text, target);
                let split = by > 0 && utf16_offset(text, floor) < target;
                let landed = if split {
                    text.ceil_char_boundary(floor + 1)
                } else {
                    floor
                };
                Some(Position::at(mtm, landed))
            })
        }

        #[unsafe(method_id(positionFromPosition:inDirection:offset:))]
        fn position_in_direction(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
            by: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            let (from, mtm) = (offset(position), self.mtm());
            let step = movement(direction, by);
            self.query(|editor| {
                let steps = by.unsigned_abs().min(editor.text().len());
                let landed = (0..steps).fold(from, |index, _| editor.moved(index, step));
                Some(Position::at(mtm, landed))
            })
        }

        #[unsafe(method(comparePosition:toPosition:))]
        fn compare(&self, position: &UITextPosition, other: &UITextPosition) -> NSComparisonResult {
            offset(position).cmp(&offset(other)).into()
        }

        #[unsafe(method(offsetFromPosition:toPosition:))]
        fn offset_between(&self, from: &UITextPosition, to: &UITextPosition) -> NSInteger {
            self.query(|editor| {
                let unit = |position| utf16_offset(editor.text(), offset(position)).cast_signed();
                Some(unit(to) - unit(from))
            })
            .unwrap_or(0)
        }

        #[unsafe(method_id(inputDelegate))]
        fn input_delegate(&self) -> Option<Retained<ProtocolObject<dyn UITextInputDelegate>>> {
            self.delegate()
        }

        #[unsafe(method(setInputDelegate:))]
        fn set_input_delegate(&self, delegate: Option<&ProtocolObject<dyn UITextInputDelegate>>) {
            *self.ivars().delegate.borrow_mut() = delegate.map(WeakObject::new);
        }

        #[unsafe(method_id(tokenizer))]
        fn tokenizer(&self) -> Retained<ProtocolObject<dyn UITextInputTokenizer>> {
            let tokenizer = self.ivars().tokenizer.get_or_init(|| unsafe {
                UITextInputStringTokenizer::initWithTextInput(
                    UITextInputStringTokenizer::alloc(self.mtm()),
                    self,
                )
            });
            ProtocolObject::from_retained(tokenizer.clone())
        }

        #[unsafe(method_id(positionWithinRange:farthestInDirection:))]
        fn farthest_position(
            &self,
            range: &UITextRange,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextPosition>> {
            let range = bytes(range);
            let edge = if forward(direction) {
                range.end
            } else {
                range.start
            };
            Some(Position::at(self.mtm(), edge))
        }

        #[unsafe(method_id(characterRangeByExtendingPosition:inDirection:))]
        fn range_by_extending(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextRange>> {
            let from = offset(position);
            let range = if forward(direction) {
                from..self.length()
            } else {
                0..from
            };
            Some(Span::over(self.mtm(), range))
        }

        #[unsafe(method(baseWritingDirectionForPosition:inDirection:))]
        fn base_writing_direction(
            &self,
            _position: &UITextPosition,
            _direction: UITextStorageDirection,
        ) -> NSWritingDirection {
            NSWritingDirection::Natural
        }

        #[unsafe(method(setBaseWritingDirection:forRange:))]
        fn set_base_writing_direction(&self, _direction: NSWritingDirection, _range: &UITextRange) {
        }

        #[unsafe(method(firstRectForRange:))]
        fn first_rect(&self, range: &UITextRange) -> CGRect {
            let range = bytes(range);
            self.locate(|editor| editor.rects(range).into_iter().next())
        }

        #[unsafe(method(caretRectForPosition:))]
        fn caret_rect(&self, position: &UITextPosition) -> CGRect {
            let from = offset(position);
            self.locate(|editor| Some(editor.caret(from)))
        }

        #[unsafe(method_id(selectionRectsForRange:))]
        fn selection_rects(&self, _range: &UITextRange) -> Retained<NSArray<UITextSelectionRect>> {
            NSArray::new()
        }

        #[unsafe(method_id(closestPositionToPoint:))]
        fn closest_position(&self, point: CGPoint) -> Option<Retained<UITextPosition>> {
            let mtm = self.mtm();
            self.at(point).map(|index| Position::at(mtm, index))
        }

        #[unsafe(method_id(closestPositionToPoint:withinRange:))]
        fn closest_position_within(
            &self,
            point: CGPoint,
            range: &UITextRange,
        ) -> Option<Retained<UITextPosition>> {
            let (range, mtm) = (bytes(range), self.mtm());
            self.at(point)
                .map(|index| Position::at(mtm, index.clamp(range.start, range.end)))
        }

        #[unsafe(method_id(characterRangeAtPoint:))]
        fn character_range(&self, point: CGPoint) -> Option<Retained<UITextRange>> {
            let (start, mtm) = (self.at(point), self.mtm());
            self.query(|editor| {
                let start = start?;
                let character = editor.slice(start..usize::MAX).chars().next();
                Some(Span::over(mtm, start..start + character.map_or(0, char::len_utf8)))
            })
        }
    }
);

impl Responder {
    fn new(
        mtm: MainThreadMarker,
        driver: &Rc<RefCell<Driver>>,
        wake: impl Fn() + 'static,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            driver: Rc::downgrade(driver),
            wake: Box::new(wake),
            delegate: RefCell::new(None),
            tokenizer: OnceCell::new(),
            known: RefCell::new(None),
            scale: Cell::new(1.0),
        });
        unsafe { msg_send![super(this), initWithFrame: CGRect::ZERO] }
    }

    fn ui<T>(&self, f: impl FnOnce(&mut Ui) -> T) -> Option<T> {
        let driver = self.ivars().driver.upgrade()?;
        let Ok(mut driver) = driver.try_borrow_mut() else {
            warn("UIKit text input reached a window that was busy; the request was dropped");
            return None;
        };
        Some(f(driver.ui()))
    }

    fn query<T>(&self, f: impl FnOnce(&Editor<'_>) -> Option<T>) -> Option<T> {
        self.ui(|ui| ui.editor().as_ref().and_then(f)).flatten()
    }

    fn hints(&self) -> InputHints {
        self.query(|editor| Some(editor.hints()))
            .unwrap_or_default()
    }

    fn delegate(&self) -> Option<Retained<ProtocolObject<dyn UITextInputDelegate>>> {
        self.ivars().delegate.borrow().as_ref()?.load()
    }

    fn length(&self) -> usize {
        self.query(|editor| Some(editor.text().len())).unwrap_or(0)
    }

    fn act(&self, f: impl FnOnce(&mut Ui)) {
        self.ui(|ui| {
            f(ui);
            Editing::track(&mut self.ivars().known.borrow_mut(), ui.editor().as_ref());
        });
        (self.ivars().wake)();
    }

    fn edit(&self, f: impl FnOnce(&Editor<'_>) -> EditTransaction) {
        self.act(|ui| {
            if let Some(transaction) = ui.editor().as_ref().map(f) {
                ui.edit_text(transaction);
            }
        });
    }

    fn press(&self, key: Key) {
        self.act(|ui| {
            ui.key_down(KeyEvent::new(key.clone()));
            ui.key_up(KeyEvent::new(key));
        });
    }

    fn points(&self, ui: &Ui) -> f64 {
        f64::from(ui.scale()) / self.ivars().scale.get()
    }

    fn locate(&self, f: impl FnOnce(&Editor<'_>) -> Option<Rect>) -> CGRect {
        self.ui(|ui| {
            let factor = self.points(ui);
            let rect = ui.editor().as_ref().and_then(f)?;
            let scaled = |value: f32| f64::from(value) * factor;
            Some(CGRect::new(
                CGPoint::new(scaled(rect.origin.x), scaled(rect.origin.y)),
                CGSize::new(scaled(rect.size.width), scaled(rect.size.height)),
            ))
        })
        .flatten()
        .unwrap_or(CGRect::ZERO)
    }

    fn at(&self, point: CGPoint) -> Option<usize> {
        self.ui(|ui| {
            let factor = self.points(ui);
            let point = Point::new((point.x / factor) as f32, (point.y / factor) as f32);
            Some(ui.editor()?.index_at(point))
        })
        .flatten()
    }

    fn publish(&self) -> bool {
        let known = &self.ivars().known;
        let previous = self
            .ui(|ui| Editing::track(&mut known.borrow_mut(), ui.editor().as_ref()))
            .flatten()
            .and_then(|Previous(previous)| previous);
        let (editing, changed) = {
            let current = known.borrow();
            let changed = previous.zip(current.as_ref()).map(|(old, new)| {
                let text = (old.id, &old.text, &old.marked) != (new.id, &new.text, &new.marked);
                (text, old.selection != new.selection, old.hints != new.hints)
            });
            (current.is_some(), changed)
        };
        let input: &ProtocolObject<dyn UITextInput> = ProtocolObject::from_ref(self);
        if let (Some((text, selection, hints)), Some(delegate)) = (changed, self.delegate()) {
            if text {
                delegate.textWillChange(Some(input));
                delegate.textDidChange(Some(input));
            } else if selection {
                delegate.selectionWillChange(Some(input));
                delegate.selectionDidChange(Some(input));
            }
            if hints {
                self.reloadInputViews();
            }
        }
        editing
    }
}

pub(crate) struct TextInput {
    host: Retained<UIView>,
    responder: Retained<Responder>,
    fallback: WinitInput,
}

impl Input for TextInput {
    fn attach(
        window: &Window,
        driver: &Rc<RefCell<Driver>>,
        waker: &EventLoopProxy<UserEvent>,
    ) -> Self {
        let mtm = MainThreadMarker::new().expect("winit runs iOS windows on the main thread");
        let host = host_view(window).expect("a winit iOS window is backed by a UIView");
        let waker = waker.clone();
        let responder = Responder::new(mtm, driver, move || {
            let _ = waker.send_event(None);
        });
        responder.setUserInteractionEnabled(false);
        let _: () = unsafe { msg_send![&*responder, setAccessibilityElementsHidden: true] };
        responder.setFrame(host.bounds());
        host.addSubview(&responder);
        let guide = host.keyboardLayoutGuide();
        responder.setTranslatesAutoresizingMaskIntoConstraints(false);
        for constraint in [
            responder
                .topAnchor()
                .constraintEqualToAnchor(&host.topAnchor()),
            responder
                .bottomAnchor()
                .constraintEqualToAnchor(&guide.topAnchor()),
            responder
                .leadingAnchor()
                .constraintEqualToAnchor(&host.leadingAnchor()),
            responder
                .trailingAnchor()
                .constraintEqualToAnchor(&host.trailingAnchor()),
        ] {
            constraint.setActive(true);
        }
        TextInput {
            host,
            responder,
            fallback: WinitInput,
        }
    }

    fn sync(&mut self, window: &Window, change: Option<TextInputChange>) {
        self.responder.ivars().scale.set(window.scale_factor());
        let editing = self.responder.publish();
        match (editing, self.responder.isFirstResponder()) {
            (true, false) => {
                window.set_ime_allowed(false);
                self.responder.becomeFirstResponder();
            }
            (false, true) => {
                self.responder.resignFirstResponder();
                self.fallback.sync(window, change);
            }
            (false, false) => self.fallback.sync(window, change),
            (true, true) => {}
        }
    }

    fn keyboard_inset(&self, _window: &Window) -> f32 {
        let covered =
            self.host.bounds().size.height - self.host.keyboardLayoutGuide().layoutFrame().origin.y;
        if covered > self.host.safeAreaInsets().bottom + 1.0 {
            covered as f32
        } else {
            0.0
        }
    }
}

fn host_view(window: &Window) -> Option<Retained<UIView>> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return None;
    };
    unsafe { Retained::retain(handle.ui_view.as_ptr().cast::<UIView>()) }
}

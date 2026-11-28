use super::round;
use super::wk_web_view::WebKitView;
use crate::engine::Layout;
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSResponder, NSView};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use raw_window_handle::RawWindowHandle;
use std::cell::Cell;
use std::ptr::null_mut;

pub(super) type View = NSView;
pub(super) type Responder = NSResponder;

pub(super) fn parent(handle: RawWindowHandle) -> Option<Retained<View>> {
    let RawWindowHandle::AppKit(handle) = handle else {
        return None;
    };
    // SAFETY: the host lends the live view of the window this page is opened in.
    unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<View>()) }
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "RougeWebContainer"]
    #[ivars = Cell<bool>]
    struct Container;

    impl Container {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: CGPoint) -> *mut NSView {
            if !self.ivars().get() {
                return null_mut();
            }
            // SAFETY: forwards AppKit's own hit test to the superclass unchanged.
            unsafe { msg_send![super(self), hitTest: point] }
        }
    }
);

impl Container {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Cell::new(true));
        // SAFETY: NSView's designated initializer, on the main thread.
        unsafe { msg_send![super(this), initWithFrame: CGRect::ZERO] }
    }
}

pub(super) struct Clip {
    parent: Retained<NSView>,
    container: Retained<Container>,
    scaled: Retained<Container>,
}

impl Clip {
    pub(super) fn new(mtm: MainThreadMarker, parent: Retained<View>, web: &WebKitView) -> Self {
        let (container, scaled) = (Container::new(mtm), Container::new(mtm));
        container.setWantsLayer(true);
        container.setHidden(true);
        scaled.addSubview(web);
        container.addSubview(&scaled);
        parent.addSubview(&container);
        Clip {
            parent,
            container,
            scaled,
        }
    }

    pub(super) fn hide(&self) {
        self.container.setHidden(true);
    }

    pub(super) fn set_interactive(&self, _web: &WebKitView, interactive: bool) {
        self.container.ivars().set(interactive);
        self.scaled.ivars().set(interactive);
        if !interactive && let Some(window) = self.parent.window() {
            window.makeFirstResponder(Some(&self.parent));
        }
    }

    pub(super) fn place(&self, web: &WebKitView, layout: &Layout) {
        let (clip, page) = (layout.clip, layout.page);
        let top = if self.parent.isFlipped() {
            f64::from(clip.origin.y)
        } else {
            self.parent.bounds().size.height - f64::from(clip.max().y)
        };
        self.container.setHidden(false);
        self.container.setFrame(CGRect::new(
            CGPoint::new(f64::from(clip.origin.x), top),
            CGSize::new(f64::from(clip.size.width), f64::from(clip.size.height)),
        ));
        let zoom = f64::from(layout.zoom);
        let (shown, unzoomed) = (
            CGSize::new(f64::from(clip.size.width), f64::from(clip.size.height)),
            CGSize::new(
                f64::from(layout.unzoomed.width),
                f64::from(layout.unzoomed.height),
            ),
        );
        self.scaled.setFrame(CGRect::new(CGPoint::ZERO, shown));
        self.scaled
            .setBoundsSize(CGSize::new(shown.width / zoom, shown.height / zoom));
        let origin = CGPoint::new(
            f64::from(page.origin.x) / zoom,
            f64::from(page.origin.y) / zoom,
        );
        if web.frame().size != unzoomed {
            web.setFrameSize(unzoomed);
        }
        web.setFrameOrigin(origin);
        if let Some(layer) = self.container.layer() {
            round(&layer, layout.corners);
        }
    }
}

impl Drop for Clip {
    fn drop(&mut self) {
        self.container.removeFromSuperview();
    }
}

use super::round;
use super::wk_web_view::WebKitView;
use crate::engine::Layout;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send};
use objc2_core_foundation::{CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_ui_kit::{UIResponder, UIView};
use raw_window_handle::RawWindowHandle;

pub(super) type View = UIView;
pub(super) type Responder = UIResponder;

pub(super) fn parent(handle: RawWindowHandle) -> Option<Retained<View>> {
    let RawWindowHandle::UiKit(handle) = handle else {
        return None;
    };
    // SAFETY: the host lends the live view of the window this page is opened in.
    unsafe { Retained::retain(handle.ui_view.as_ptr().cast::<View>()) }
}

pub(super) struct Clip {
    container: Retained<UIView>,
}

impl Clip {
    pub(super) fn new(mtm: MainThreadMarker, parent: Retained<View>, web: &WebKitView) -> Self {
        // SAFETY: UIView's designated initializer, on the main thread.
        let container: Retained<UIView> =
            unsafe { msg_send![UIView::alloc(mtm), initWithFrame: CGRect::ZERO] };
        container.setHidden(true);
        container.addSubview(web);
        parent.addSubview(&container);
        Clip { container }
    }

    pub(super) fn hide(&self) {
        self.container.setHidden(true);
    }

    pub(super) fn set_interactive(&self, web: &WebKitView, interactive: bool) {
        self.container.setUserInteractionEnabled(interactive);
        if !interactive {
            web.end_editing(true);
        }
    }

    pub(super) fn place(&self, web: &WebKitView, layout: &Layout) {
        let (clip, page, zoom) = (layout.clip, layout.page, f64::from(layout.zoom));
        let unzoomed = layout.unzoomed;
        self.container.setHidden(false);
        self.container.setFrame(CGRect::new(
            CGPoint::new(f64::from(clip.origin.x), f64::from(clip.origin.y)),
            CGSize::new(f64::from(clip.size.width), f64::from(clip.size.height)),
        ));
        web.setBounds(CGRect::new(
            CGPoint::ZERO,
            CGSize::new(f64::from(unzoomed.width), f64::from(unzoomed.height)),
        ));
        web.setCenter(CGPoint::new(
            f64::from(page.origin.x + page.size.width / 2.0),
            f64::from(page.origin.y + page.size.height / 2.0),
        ));
        web.setTransform(CGAffineTransform {
            a: zoom,
            b: 0.0,
            c: 0.0,
            d: zoom,
            tx: 0.0,
            ty: 0.0,
        });
        round(&self.container.layer(), layout.corners);
    }
}

impl Drop for Clip {
    fn drop(&mut self) {
        self.container.removeFromSuperview();
    }
}

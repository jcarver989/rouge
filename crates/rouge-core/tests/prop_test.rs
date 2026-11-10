use rouge_core::testing::TestUi;
use rouge_core::{BoxShadow, Color, Corners, Styled, div, pct, px};
use rouge_reactive::{Runtime, memo, signal};

#[test]
fn props_accept_values_closures_signals_and_memos() {
    let _rt = Runtime::new().enter();
    let color = signal(Color::RED);
    let width = signal(10.0f32);
    let wide = memo(move || width.get() * 2.0);
    let mut ui = TestUi::new(|| {
        div()
            .bg(Color::RED)
            .bg(0xff0000)
            .bg(color)
            .bg(move || color.get())
            .bg(move || 0x00ff00)
            .w(10.0)
            .w(10)
            .w(px(10.0))
            .w(pct(50.0))
            .w(width)
            .w(wide)
            .w(move || width.get() + 1.0)
            .rounded(4.0)
            .rounded(Corners::all(4.0))
            .rounded(move || width.get())
            .shadow(BoxShadow::md())
            .flex_grow(1.0)
    });
    assert_eq!(ui.stats().elements, 2);
}

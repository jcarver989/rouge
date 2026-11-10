use super::palette::{ACCENT, BASE, MUTED, RAISED, SURFACE, TEXT};
use rouge::prelude::*;

const ROWS: usize = 100_000;

fn panel(title: &'static str) -> Div {
    div()
        .flex()
        .col()
        .gap(12.0)
        .p(16.0)
        .rounded(12.0)
        .bg(SURFACE)
        .child(text(title).text_size(13.0).weight(600.0).color(MUTED))
}

fn rows() -> impl View {
    let row = |index: usize| {
        div()
            .size_full()
            .flex()
            .items_center()
            .px(12.0)
            .bg(if index.is_multiple_of(2) {
                SURFACE
            } else {
                Color::rgb(0x2a2b3c)
            })
            .child(text(format!("Row {index}")).text_size(13.0).color(TEXT))
    };
    panel("100,000 virtual rows")
        .flex_1()
        .child(virtual_list(|| ROWS, 28.0, row).flex_1())
}

fn cards() -> impl View {
    let next = signal(4u32);
    let items = signal(vec![1u32, 2, 3]);
    let add = move |_: &PointerEvent| {
        let id = next.get_untracked();
        next.set(id + 1);
        items.update(|items| items.insert(0, id));
    };
    let card = move |id: ReadSignal<u32>| {
        let id = id.get_untracked();
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(40.0)
            .px(12.0)
            .rounded(8.0)
            .bg(RAISED)
            .hover(|s| s.bg(0x585b70))
            .layout_transition(Spring::snappy())
            .on_click(move |_| items.update(|items| items.retain(|other| *other != id)))
            .child(text(format!("Card {id}")).text_size(14.0).color(TEXT))
            .child(text("click to remove").text_size(12.0).color(MUTED))
    };
    let list = each(move || items.get(), |id| *id, card)
        .enter(Pose::new().opacity(0.0).scale(0.9), Spring::snappy())
        .exit(Pose::new().opacity(0.0).translate_x(48.0), Tween::ms(200.0));
    let add = div()
        .flex()
        .items_center()
        .justify_center()
        .h(32.0)
        .rounded(8.0)
        .bg(ACCENT)
        .on_click(add)
        .child(text("Add card").text_size(13.0).weight(600.0).color(BASE));
    panel("Enter, exit and layout transitions")
        .child(add)
        .child(div().flex().col().gap(8.0).child(list))
}

fn switch() -> impl View {
    let on = signal(false);
    let x = motion(0.0);
    let toggle = move |_: &PointerEvent| {
        on.update(|on| *on = !*on);
        x.animate_to(
            if on.get_untracked() { 28.0 } else { 0.0 },
            Spring::bouncy(),
        );
    };
    let knob = div()
        .size(24.0)
        .m(2.0)
        .rounded(12.0)
        .bg(Color::WHITE)
        .translate_x(x);
    div()
        .w(56.0)
        .h(28.0)
        .rounded(14.0)
        .bg(move || if on.get() { ACCENT } else { RAISED })
        .on_click(toggle)
        .child(knob)
}

fn spinner() -> impl View {
    let spin = Keyframes::new()
        .at(0.0, |pose| pose.rotate(0.0))
        .at(1.0, |pose| pose.rotate(360.0))
        .duration(1.2)
        .repeat();
    div().size(22.0).rounded(5.0).bg(ACCENT).animate(spin)
}

pub(crate) fn motion_demo() -> impl View {
    let controls = panel("Motion value and keyframes").child(
        div()
            .flex()
            .items_center()
            .gap(20.0)
            .child(switch())
            .child(spinner()),
    );
    div()
        .size_full()
        .flex()
        .gap(16.0)
        .p(16.0)
        .bg(BASE)
        .child(rows())
        .child(
            div()
                .flex()
                .col()
                .gap(16.0)
                .w(300.0)
                .child(controls)
                .child(cards()),
        )
}

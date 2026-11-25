mod common;

use common::{builder, handheld_test, viewport};
use rouge_core::access;
use rouge_core::testing::{TestResult, counter};
use rouge_core::view;
use rouge_core::{Color, Div, IntoChord, Point, Rect, Role, Size, Styled, Theme, div, show, text};
use rouge_reactive::{Runtime, signal};
use rouge_theme::UiPalette;
use rouge_widgets::testing::component_test;
use rouge_widgets::{Hint, Material, Placement, hover_card, popover, tooltip};
use std::thread::sleep;
use std::time::Duration;

#[test]
fn hover_card_floats_below_its_anchor_without_blocking_the_page() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(true);
    let anchor = signal(Rect::new(20.0, 20.0, 40.0, 16.0));
    let hovered = signal(false);
    let (behind, hit_behind) = counter();
    let mut ui = builder().build(move || {
        let card = hover_card(
            move || open.get(),
            anchor,
            move || {
                div()
                    .test_id("card")
                    .size(80.0)
                    .on_hover(move |inside| hovered.set(inside))
            },
        );
        viewport()
            .child(
                div()
                    .test_id("behind")
                    .absolute()
                    .right(0.0)
                    .bottom(0.0)
                    .size(40.0)
                    .on_click(move |_| hit_behind()),
            )
            .child(card)
    });
    ui.run(0.6);
    let card = ui.rect_of("card")?;
    assert_eq!(card.origin.x, anchor.get().origin.x);
    assert!(card.origin.y > anchor.get().max().y);
    ui.hover_on("card")?;
    assert!(hovered.get(), "the card itself takes hover");
    ui.click_on("behind")?;
    assert_eq!(behind.get(), 1, "the page behind stays interactive");
    assert!(open.get(), "only its owner closes it");
    anchor.set(Rect::new(100.0, 10.0, 20.0, 16.0));
    ui.run(0.1);
    let moved = ui.rect_of("card")?;
    assert_eq!(
        moved.origin.x,
        anchor.get().origin.x,
        "it follows its anchor"
    );
    assert!(moved.origin.y > anchor.get().max().y);
    open.set(false);
    ui.run(0.6);
    assert_eq!(ui.bounds_of("card"), None);
    Ok(())
}

#[test]
fn popover_opens_below_its_trigger_and_closes_on_outside_click_or_escape() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(false);
    let (inside, hit_inside) = counter();
    let (behind, hit_behind) = counter();
    let mut ui = builder().build(move || {
        let content = move || {
            let hit_inside = hit_inside.clone();
            div()
                .test_id("panel")
                .size(80.0)
                .on_click(move |_| hit_inside())
        };
        viewport()
            .p(10.0)
            .child(popover(open, trigger(), content))
            .child(
                div()
                    .absolute()
                    .right(0.0)
                    .bottom(0.0)
                    .test_id("behind")
                    .size(50.0)
                    .on_click(move |_| hit_behind()),
            )
    });
    ui.click_on("trigger")?;
    ui.run(0.6);
    let panel = ui.rect_of("panel")?;
    let trigger = ui.rect_of("trigger")?;
    assert_eq!(panel.origin.x, trigger.origin.x);
    assert!(panel.origin.y > trigger.max().y, "below the trigger");
    ui.click_on("panel")?;
    assert_eq!(inside.get(), 1);
    assert!(open.get(), "clicks inside keep it open");
    ui.click_on("behind")?;
    assert!(!open.get(), "an outside click dismisses it");
    assert_eq!(behind.get(), 0, "and is swallowed");
    ui.run(0.6);
    ui.click_on("behind")?;
    assert_eq!(behind.get(), 1, "the page is interactive again");

    ui.click_on("trigger")?;
    ui.run(0.6);
    assert!(open.get());
    assert!(
        ui.press("escape"),
        "focus moved into the popover, so escape reaches it"
    );
    assert!(!open.get());
    ui.run(0.6);
    assert!(ui.press("enter"), "focus returned to the trigger");
    assert!(open.get());
    Ok(())
}

#[test]
fn popover_can_be_placed_on_any_side_of_its_trigger() -> TestResult {
    let _entered = Runtime::new().enter();
    for placement in [
        Placement::Below,
        Placement::Above,
        Placement::Right,
        Placement::Left,
    ] {
        let mut ui = builder().build(move || {
            viewport().flex().items_center().justify_center().child(
                popover(signal(true), trigger(), || {
                    div().test_id("panel").size(40.0)
                })
                .placement(placement),
            )
        });
        ui.run(0.6);
        let panel = ui.rect_of("panel")?;
        let trigger = ui.rect_of("trigger")?;
        let gap = match placement {
            Placement::Below => panel.origin.y - trigger.max().y,
            Placement::Above => trigger.origin.y - panel.max().y,
            Placement::Right => panel.origin.x - trigger.max().x,
            Placement::Left => trigger.origin.x - panel.max().x,
        };
        assert!(
            gap > 0.0,
            "panel is clear of the trigger on its {placement:?} side"
        );
    }
    Ok(())
}

#[test]
fn popover_keeps_its_preferred_side_when_it_only_needs_to_shift() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        viewport().child(
            popover(signal(true), trigger().size(20.0), || {
                div().test_id("panel").w(80.0).h(60.0)
            })
            .absolute()
            .right(0.0)
            .top(100.0),
        )
    });
    ui.run(0.6);
    let trigger = ui.rect_of("trigger")?;
    let panel = ui.rect_of("panel")?;
    let viewport = ui.rect_of("viewport")?;
    assert!(panel.origin.y > trigger.max().y, "stays below the trigger");
    assert!(panel.origin.x < trigger.origin.x, "shifts left");
    assert!(panel.max().x <= viewport.max().x, "stays inside the window");
    Ok(())
}

#[test]
fn an_oversized_popover_scrolls_on_the_roomier_side_and_returns_below_after_resize() -> TestResult {
    let _entered = Runtime::new().enter();
    let (open, clicked) = (signal(true), signal(false));
    let mut ui = builder().build(move || {
        viewport().child(
            popover(open, trigger(), move || {
                div().w(80.0).child(div().test_id("first").h(120.0)).child(
                    div()
                        .test_id("last")
                        .h(20.0)
                        .on_click(move |_| clicked.set(true)),
                )
            })
            .material(Material::Bare)
            .absolute()
            .left(0.0)
            .top(120.0),
        )
    });
    ui.run(0.6);
    let trigger = ui.rect_of("trigger")?;
    let first = ui.visible_rect_of("first")?;
    let last = ui.rect_of("last")?;
    assert!(!first.is_empty());
    assert!(
        first.max().y < trigger.origin.y,
        "opens above without covering the trigger"
    );
    assert!(first.size.height < ui.rect_of("first")?.size.height);
    assert!(
        ui.visible_rect_of("last")?.is_empty(),
        "overflow is clipped"
    );
    ui.click_on("trigger")?;
    assert!(!clicked.get(), "overflow must not intercept the trigger");
    assert!(!open.get(), "the trigger lies outside the panel");

    ui.run(0.6);
    ui.click_on("trigger")?;
    ui.run(0.6);
    ui.scroll_on("first", Point::new(0.0, first.max().y - last.max().y))?;
    assert_eq!(ui.visible_rect_of("last")?, ui.rect_of("last")?);
    ui.click_on("last")?;
    assert!(
        clicked.get(),
        "the clipped content is reachable by scrolling"
    );

    let window = ui.rect_of("viewport")?.size;
    ui.set_size(Size::new(window.width, window.height * 2.0));
    assert!(ui.rect_of("first")?.origin.y > ui.rect_of("trigger")?.max().y);
    Ok(())
}

#[test]
fn an_oversized_side_popover_stays_clear_of_its_trigger_and_scrolls_horizontally() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        viewport().child(
            popover(signal(true), trigger().size(20.0), || {
                div().test_id("content").w(200.0).h(40.0)
            })
            .placement(Placement::Right)
            .material(Material::Bare)
            .absolute()
            .right(100.0)
            .bottom(0.0),
        )
    });
    ui.run(0.6);
    let trigger = ui.rect_of("trigger")?;
    let content = ui.rect_of("content")?;
    let visible = ui.visible_rect_of("content")?;
    assert!(!visible.is_empty());
    assert!(visible.max().x < trigger.origin.x, "flips left");
    assert!(
        visible.size.width < content.size.width,
        "overflow is clipped"
    );
    assert_eq!(
        visible.size.height, content.size.height,
        "shifts up into the window"
    );

    ui.scroll_on(
        "content",
        Point::new(visible.max().x - content.max().x, 0.0),
    )?;
    assert!(ui.rect_of("content")?.origin.x < content.origin.x);
    assert_eq!(
        ui.visible_rect_of("content")?.max().x,
        ui.rect_of("content")?.max().x
    );
    Ok(())
}

#[test]
fn a_glass_popover_lets_what_is_behind_it_show_through_its_panel() -> TestResult {
    let _entered = Runtime::new().enter();
    let panel = |material: Material| {
        let open = signal(true);
        let mut ui = builder().build(move || {
            viewport().bg(Color::RED).child(
                popover(open, trigger(), || div().test_id("panel").size(80.0)).material(material),
            )
        });
        ui.run(0.6);
        let point = ui.center_of("panel")?;
        ui.pixel(point.x as u32, point.y as u32)
    };
    let [red, green, ..] = panel(Material::Surface)?;
    assert_eq!(red, green, "a solid surface hides the page");
    let [red, green, ..] = panel(Material::Glass)?;
    assert!(red > green, "glass shows the page through its tint");
    Ok(())
}

#[test]
fn tooltip_appears_after_hovering_for_the_delay_and_hides_on_leave() -> TestResult {
    let _entered = Runtime::new().enter();
    let delay = Duration::from_millis(20);
    let mut ui = builder().build(|| {
        viewport()
            .flex()
            .col()
            .child(tooltip(trigger(), "Helpful").delay(delay))
            .child(div().test_id("outside").flex_1())
    });
    ui.hover_on("trigger")?;
    assert!(ui.text_bounds("Helpful").is_none(), "nothing yet");
    assert!(ui.wait_until(|ui| ui.text_bounds("Helpful").is_some()));
    ui.hover_on("outside")?;
    assert!(ui.wait_until(|ui| ui.text_bounds("Helpful").is_none()));

    ui.hover_on("trigger")?;
    ui.hover_on("outside")?;
    sleep(delay * 2);
    ui.run(0.6);
    assert!(
        ui.text_bounds("Helpful").is_none(),
        "leaving before the delay cancels it"
    );
    Ok(())
}

#[test]
fn any_view_takes_an_optional_hint_and_none_adds_nothing() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        viewport()
            .child(trigger().hint(Some("Helpful")))
            .child(trigger().test_id("plain").hint(None::<&str>))
    });
    let idle = ui.stats().elements;
    ui.hover_on("plain")?;
    ui.run(0.6);
    assert_eq!(ui.stats().elements, idle, "no hint, no tooltip");
    ui.hover_on("trigger")?;
    assert!(ui.wait_until(|ui| ui.text_bounds("Helpful").is_some()));
    Ok(())
}

#[test]
fn a_tooltip_or_popover_allowed_to_shrink_truncates_its_target() -> TestResult {
    let _entered = Runtime::new().enter();
    const RIGID_WIDTH: f32 = 60.0;
    let mut ui = builder().build(|| {
        let label = |id: &str| {
            div()
                .test_id(id)
                .min_w(0.0)
                .overflow_hidden()
                .child(text("A label far too long for the row it sits in").no_wrap())
        };
        let fixed = |id: &str| div().test_id(id).flex_none().w(RIGID_WIDTH).h(20.0);
        let row = |id: &str| div().test_id(id).w(200.0).flex();
        div()
            .size_full()
            .child(
                row("hinted")
                    .child(label("hinted-label").tooltip("Helpful").min_w(0.0))
                    .child(fixed("hinted-fixed").tooltip("Helpful")),
            )
            .child(
                row("popped")
                    .child(popover(signal(false), label("popped-label"), div).min_w(0.0))
                    .child(popover(signal(false), fixed("popped-fixed"), div)),
            )
    });
    for row in ["hinted", "popped"] {
        let [label, fixed] = ["label", "fixed"].map(|part| ui.rect_of(&format!("{row}-{part}")));
        let (label, fixed) = (label?, fixed?);
        assert_eq!(
            fixed.size.width, RIGID_WIDTH,
            "{row}: a rigid target keeps its size"
        );
        assert_eq!(
            label.size.width,
            ui.rect_of(row)?.size.width - fixed.size.width,
            "{row}: a shrinkable target takes what is left"
        );
        assert_eq!(
            fixed.origin.x,
            label.max().x,
            "{row}: and stays clear of its neighbour"
        );
    }
    Ok(())
}

#[test]
fn tooltip_takes_the_surface_of_a_dark_theme() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        Theme::provide(UiPalette::dark());
        viewport()
            .p(10.0)
            .bg(Role::Canvas)
            .child(tooltip(trigger(), "Helpful").delay(Duration::from_millis(20)))
    });
    ui.hover_on("trigger")?;
    assert!(ui.wait_until(|ui| ui.text_bounds("Helpful").is_some()));
    ui.run(0.5);
    let label = ui
        .text_bounds("Helpful")
        .ok_or("the tooltip label is drawn")?;
    let bubble = ui.pixel((label.origin.x - 2.0) as u32, label.center().y as u32)?;
    let distance = |color: Color| {
        let target = color.to_rgba8();
        bubble
            .iter()
            .zip(target)
            .map(|(a, b)| u32::from(a.abs_diff(b)))
            .sum::<u32>()
    };
    let palette = UiPalette::dark().map(Color::from);
    assert!(
        distance(palette.elevated) < distance(palette.text),
        "the bubble is dark glass behind light text, not inverted: {bubble:?}"
    );
    Ok(())
}

#[test]
fn a_tooltip_shows_a_shortcut_in_a_key_cap_or_rich_content() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(300.0, 200.0).build(|| {
        let plain = view! {
            <div test_id="shortcut-trigger" w=60.0 h=20.0 tooltip="Copy link" shortcut="secondary-c" delay=Duration::from_millis(1)/>
        };
        let rich = view! {
            <div test_id="rich-trigger" w=60.0 h=20.0 tooltip="Soft Machinery"
                 content={|| view! { <div test_id="rich" size=40.0/> }} delay=Duration::from_millis(1)/>
        };
        view! {
            <div size_full p=10.0 flex gap=40.0>
                {plain}
                {rich}
            </div>
        }
    });
    ui.hover_on("shortcut-trigger")?;
    assert!(ui.wait_until(|ui| ui.text_bounds("Copy link").is_some()));
    let label = ui.text_bounds("Copy link").ok_or("no label")?;
    let key = ui
        .text_bounds(&"secondary-c".into_chord().display())
        .ok_or("no shortcut")?;
    assert!(
        key.origin.x >= label.max().x,
        "the key cap follows the label"
    );
    ui.hover_on("rich-trigger")?;
    assert!(ui.wait_until(|ui| ui.bounds_of("rich").is_some()));
    assert!(
        ui.text_bounds("Soft Machinery").is_none(),
        "content replaces the caption"
    );
    Ok(())
}

#[test]
fn a_popover_scales_in_from_its_trigger_and_then_idles() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(false);
    let mut ui = builder().build(move || {
        let trigger = view! { <div w=60.0 h=24.0/> };
        let content = || view! { <div test_id="panel" size=80.0 bg=Color::RED/> };
        view! {
            <div size_full p=10.0>
                <popover open=open trigger=trigger children=content material=Material::Bare/>
            </div>
        }
    });
    open.set(true);
    ui.advance(0.0);
    assert!(ui.is_animating());
    ui.advance(1.0 / 60.0);
    let panel = ui.rect_of("panel")?;
    let (x, y) = (panel.origin.x as u32, panel.center().y as u32);
    assert_ne!(
        ui.pixel(x, y)?,
        Color::RED.to_rgba8(),
        "it starts smaller and faint"
    );
    ui.run(1.0);
    assert_eq!(
        ui.pixel(x, y)?,
        Color::RED.to_rgba8(),
        "and settles at full size"
    );
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

#[test]
fn tooltip_unmounted_while_pending_never_opens() -> TestResult {
    let _entered = Runtime::new().enter();
    let mounted = signal(true);
    let mut ui = builder().build(move || {
        div().size_full().p(10.0).child(show(
            move || mounted.get(),
            || tooltip(trigger(), "Helpful").delay(Duration::from_millis(20)),
        ))
    });
    ui.hover_on("trigger")?;
    mounted.set(false);
    ui.advance(0.0);
    sleep(Duration::from_millis(40));
    ui.run(0.6);
    assert!(ui.text_bounds("Helpful").is_none());
    Ok(())
}

#[test]
fn a_popover_is_a_bottom_sheet_on_a_handheld() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(false);
    let mut ui = handheld_test().build(move || {
        let trigger = view! { <div test_id="trigger" size=40.0/> };
        view! {
            <div size_full p=20.0 test_id="viewport">
                <popover open=open trigger=trigger>
                    <div test_id="panel" h=200.0/>
                </popover>
            </div>
        }
    });
    ui.tap_on("trigger")?;
    assert!(open.get());
    ui.run(2.0);
    let panel = ui.rect_of("panel")?;
    let window = ui.rect_of("viewport")?;
    assert_eq!(panel.origin.x, window.origin.x);
    assert_eq!(panel.size.width, window.size.width);
    assert_eq!(panel.max().y, window.max().y, "docks to the bottom edge");
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    let grip = ui
        .access_node(access::Role::Splitter, "Sheet height")?
        .bounds
        .center();
    ui.swipe_over(grip, Point::new(grip.x, panel.center().y), 0.3);
    ui.run(2.0);
    assert!(!open.get(), "dragging it off the bottom closes it");
    assert_eq!(ui.bounds_of("panel"), None);
    ui.set_size(Size::new(window.size.width * 2.0, window.size.height));
    open.set(true);
    ui.run(2.0);
    let trigger = ui.rect_of("trigger")?;
    let panel = ui.rect_of("panel")?;
    assert_eq!(panel.origin.x, trigger.origin.x);
    assert!(
        panel.origin.y > trigger.max().y,
        "at a regular width it is anchored again"
    );
    Ok(())
}

fn trigger() -> Div {
    div().test_id("trigger").w(60.0).h(20.0)
}

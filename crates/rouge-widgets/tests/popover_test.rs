mod common;

use common::{builder, handheld_test};
use rouge_core::testing::{TestResult, TestUi, counter};
use rouge_core::view;
use rouge_core::{Color, IntoChord, Point, Rect, Role, Size, Styled, Theme, div, show, text};
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
        div()
            .size_full()
            .on_click(move |_| hit_behind())
            .child(card)
    });
    ui.run(0.6);
    assert_eq!(
        ui.bounds_of("card"),
        Some(Rect::new(20.0, 40.0, 80.0, 80.0)),
        "just below the anchor"
    );
    ui.pointer_moved(Point::new(40.0, 60.0));
    assert!(hovered.get(), "the card itself takes hover");
    ui.click_at(150.0, 150.0);
    assert_eq!(behind.get(), 1, "the page behind stays interactive");
    assert!(open.get(), "only its owner closes it");
    anchor.set(Rect::new(100.0, 10.0, 20.0, 16.0));
    ui.run(0.1);
    assert_eq!(
        ui.bounds_of("card").map(|card| card.origin),
        Some(Point::new(100.0, 30.0)),
        "it follows its anchor"
    );
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
        let trigger = div().w(60.0).h(24.0);
        let content = move || {
            let hit_inside = hit_inside.clone();
            div().size(80.0).on_click(move |_| hit_inside())
        };
        div()
            .size_full()
            .p(10.0)
            .child(popover(open, trigger, content))
            .child(
                div()
                    .absolute()
                    .left(200.0)
                    .top(100.0)
                    .size(50.0)
                    .on_click(move |_| hit_behind()),
            )
    });
    ui.click_at(20.0, 20.0);
    ui.run(0.6);
    assert_eq!(
        ui.bounds(30.0, 60.0)?,
        Rect::new(10.0, 38.0, 80.0, 80.0),
        "below the trigger"
    );
    ui.click_at(30.0, 60.0);
    assert_eq!(inside.get(), 1);
    assert!(open.get(), "clicks inside keep it open");
    ui.click_at(220.0, 120.0);
    assert!(!open.get(), "an outside click dismisses it");
    assert_eq!(behind.get(), 0, "and is swallowed");
    ui.run(0.6);
    ui.click_at(220.0, 120.0);
    assert_eq!(behind.get(), 1, "the page is interactive again");

    ui.click_at(20.0, 20.0);
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
    let sides = [
        (Placement::Above, 50.0, 76.0),
        (Placement::Right, 114.0, 120.0),
        (Placement::Left, 6.0, 120.0),
    ];
    for (placement, x, y) in sides {
        let open = signal(true);
        let mut ui = builder().build(move || {
            let trigger = div().w(60.0).h(20.0);
            let anchored = popover(open, trigger, || div().size(40.0)).placement(placement);
            div()
                .size_full()
                .child(div().absolute().left(50.0).top(120.0).child(anchored))
        });
        ui.run(0.6);
        assert_eq!(
            ui.bounds(x + 20.0, y + 20.0)?,
            Rect::new(x, y, 40.0, 40.0),
            "{placement:?} sits a gap away from the trigger"
        );
    }
    Ok(())
}

#[test]
fn a_glass_popover_lets_what_is_behind_it_show_through_its_panel() -> TestResult {
    let _entered = Runtime::new().enter();
    let panel = |material: Material| {
        let open = signal(true);
        let mut ui = builder().build(move || {
            let trigger = div().w(60.0).h(20.0);
            div()
                .size_full()
                .bg(Color::RED)
                .child(popover(open, trigger, || div().size(80.0)).material(material))
        });
        ui.run(0.6);
        ui.pixel(40, 60)
    };
    let [red, green, ..] = panel(Material::Surface)?;
    assert_eq!(red, green, "a solid surface hides the page");
    let [red, green, ..] = panel(Material::Glass)?;
    assert!(red > green, "glass shows the page through its tint");
    Ok(())
}

#[test]
fn tooltip_appears_after_hovering_for_the_delay_and_hides_on_leave() {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        let target = div().w(60.0).h(20.0);
        div()
            .size_full()
            .p(10.0)
            .child(tooltip(target, "Helpful").delay(Duration::from_millis(20)))
    });
    let shown = |ui: &mut TestUi| ui.stats().elements;
    let idle = shown(&mut ui);
    ui.pointer_moved(Point::new(20.0, 20.0));
    ui.advance(0.0);
    assert_eq!(shown(&mut ui), idle, "nothing yet");
    assert!(
        ui.wait_until(|ui| shown(ui) > idle),
        "the tooltip mounts after the delay"
    );
    ui.pointer_moved(Point::new(200.0, 150.0));
    assert!(
        ui.wait_until(|ui| shown(ui) == idle),
        "and unmounts after leaving"
    );
    ui.pointer_moved(Point::new(20.0, 20.0));
    ui.pointer_moved(Point::new(200.0, 150.0));
    sleep(Duration::from_millis(40));
    ui.run(0.6);
    assert_eq!(shown(&mut ui), idle, "leaving before the delay cancels it");
}

#[test]
fn any_view_takes_an_optional_hint_and_none_adds_nothing() {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        let hinted = div().w(60.0).h(20.0).hint(Some("Helpful"));
        let plain = div().w(60.0).h(20.0).hint(None::<&str>);
        div().size_full().child(hinted).child(plain)
    });
    let shown = |ui: &mut TestUi| ui.stats().elements;
    let idle = shown(&mut ui);
    ui.pointer_moved(Point::new(20.0, 30.0));
    sleep(Duration::from_millis(40));
    ui.run(0.6);
    assert_eq!(shown(&mut ui), idle, "no hint, no tooltip");
    ui.pointer_moved(Point::new(20.0, 10.0));
    assert!(
        ui.wait_until(|ui| shown(ui) > idle),
        "the hinted view shows one"
    );
}

#[test]
fn a_tooltip_or_popover_allowed_to_shrink_truncates_its_target() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        let label = |id: &str| {
            div()
                .test_id(id)
                .min_w(0.0)
                .overflow_hidden()
                .child(text("A label far too long for the row it sits in").no_wrap())
        };
        let fixed = |id: &str| div().test_id(id).flex_none().w(60.0).h(20.0);
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
            fixed.size.width, 60.0,
            "{row}: a rigid target keeps its size"
        );
        assert_eq!(
            label.size.width, 140.0,
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
        let target = div().w(60.0).h(20.0);
        div()
            .size_full()
            .p(10.0)
            .bg(Role::Canvas)
            .child(tooltip(target, "Helpful").delay(Duration::from_millis(20)))
    });
    let idle = ui.stats().elements;
    ui.pointer_moved(Point::new(20.0, 20.0));
    assert!(
        ui.wait_until(|ui| ui.stats().elements > idle),
        "the tooltip mounts"
    );
    ui.run(0.5);
    let bubble = ui.pixel(14, 46)?;
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
            <div w=60.0 h=20.0 tooltip="Copy link" shortcut="secondary-c" delay=Duration::from_millis(1)/>
        };
        let rich = view! {
            <div w=60.0 h=20.0 tooltip="Soft Machinery"
                 content={|| view! { <div test_id="rich" size=40.0/> }} delay=Duration::from_millis(1)/>
        };
        view! {
            <div size_full p=10.0 flex gap=40.0>
                {plain}
                {rich}
            </div>
        }
    });
    ui.pointer_moved(Point::new(20.0, 20.0));
    assert!(ui.wait_until(|ui| ui.text_bounds("Copy link").is_some()));
    let label = ui.text_bounds("Copy link").ok_or("no label")?;
    let key = ui
        .text_bounds(&"secondary-c".into_chord().display())
        .ok_or("no shortcut")?;
    assert!(
        key.origin.x >= label.max().x,
        "the key cap follows the label"
    );
    ui.pointer_moved(Point::new(120.0, 20.0));
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
        [255, 0, 0, 255],
        "it starts smaller and faint"
    );
    ui.run(1.0);
    assert_eq!(
        ui.pixel(x, y)?,
        [255, 0, 0, 255],
        "and settles at full size"
    );
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

#[test]
fn tooltip_unmounted_while_pending_never_opens() {
    let _entered = Runtime::new().enter();
    let mounted = signal(true);
    let mut ui = builder().build(move || {
        div().size_full().p(10.0).child(show(
            move || mounted.get(),
            || tooltip(div().w(60.0).h(20.0), "Helpful").delay(Duration::from_millis(20)),
        ))
    });
    ui.pointer_moved(Point::new(20.0, 20.0));
    ui.advance(0.0);
    mounted.set(false);
    ui.advance(0.0);
    sleep(Duration::from_millis(40));
    ui.run(0.6);
}

#[test]
fn a_popover_is_a_bottom_sheet_on_a_handheld() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(false);
    let mut ui = handheld_test().build(move || {
        let trigger = view! { <div test_id="trigger" size=40.0/> };
        view! {
            <div size_full p=20.0>
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
    assert_eq!(
        (panel.origin.x, panel.size.width, panel.max().y),
        (0.0, 390.0, 700.0),
        "the content spans the bottom edge instead of hanging off its trigger"
    );
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    let grip = Point::new(195.0, panel.origin.y + 40.0);
    ui.swipe_over(grip, Point::new(195.0, 690.0), 0.3);
    ui.run(2.0);
    assert!(!open.get(), "dragging it off the bottom closes it");
    assert_eq!(ui.bounds_of("panel"), None);
    ui.set_size(Size::new(900.0, 700.0));
    open.set(true);
    ui.run(2.0);
    assert_eq!(
        ui.rect_of("panel")?.origin,
        Point::new(20.0, 64.0),
        "at a regular width it is anchored again"
    );
    Ok(())
}

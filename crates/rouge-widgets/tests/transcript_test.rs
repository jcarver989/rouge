#![cfg(feature = "markdown")]

use rouge_core::testing::{TestResult, assert_near, counter};
use rouge_core::view;
use rouge_core::{Color, Styled, UiPalette, div};
use rouge_markdown::MarkdownStream;
use rouge_reactive::{Runtime, keyed_list, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{Message, composer, transcript};

#[test]
fn a_transcript_draws_bubbles_and_follows_a_growing_reply_to_the_end() -> TestResult {
    let _entered = Runtime::new().enter();
    let reply = signal(MarkdownStream::new());
    let thread = keyed_list(|(id, _): &(u32, Message)| *id);
    thread.push((0, Message::user("Play something quiet")));
    thread.push((1, Message::assistant(reply)));
    let following = signal(true);
    let mut ui = component_test()
        .size(400.0, 200.0)
        .clear(Color::WHITE)
        .build(move || {
            let list = view! {
                <transcript messages=thread message={|(_, message)| message.clone()} test_id="thread"
                     following=following flex_1/>
            };
            view! { <div size_full p=10.0 flex col>{list}</div> }
        });
    ui.run(0.3);
    let asked = ui
        .text_bounds("Play something quiet")
        .ok_or("the user bubble is drawn")?;
    assert!(
        asked.center().x > 200.0,
        "what the user said sits on the right"
    );
    let accent = UiPalette::light().map(Color::from).accent.to_rgba8();
    assert_near(
        ui.pixel(asked.origin.x as u32 - 6, asked.center().y as u32)?,
        accent,
        "in a flat accent bubble",
    );
    reply.update(|reply| {
        reply.push("Here is **Soft Machinery**.\n\n");
    });
    ui.run(0.3);
    let answer = ui
        .text_bounds("Soft Machinery")
        .ok_or("the reply is drawn")?;
    assert!(answer.origin.x < 200.0 && answer.origin.y > asked.max().y);
    for line in 0..40 {
        reply.update(|reply| {
            reply.push(&format!("Track {line} is next.\n\n"));
        });
        ui.advance(1.0 / 60.0);
    }
    ui.run(0.5);
    assert!(following.get());
    let last = ui
        .text_bounds("Track 39")
        .ok_or("the end of the reply is in view")?;
    assert!(last.max().y <= 200.0);
    assert!(
        ui.text_bounds("Play something quiet")
            .is_none_or(|asked| asked.max().y < 10.0),
        "the top scrolled away"
    );
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

#[test]
fn a_composer_sends_on_enter_or_its_button_and_morphs_into_stop_while_busy() -> TestResult {
    let _entered = Runtime::new().enter();
    let (draft, busy, sent) = (signal(String::new()), signal(false), signal(Vec::new()));
    let (stops, stop) = counter();
    let mut ui = component_test().size(400.0, 200.0).build(move || {
        view! {
            <div p=10.0>
                <composer draft=draft test_id="composer" placeholder="Ask for something to play" busy=busy
                     on_send={move |said: String| sent.update(|sent| sent.push(said))}
                     on_stop=stop w=360.0/>
            </div>
        }
    });
    let idle_action = ui.rect_of("composer-send")?;
    ui.click_on("composer-send")?;
    assert!(sent.get().is_empty(), "an empty draft is not sent");
    ui.click_on("composer-input")?;
    ui.type_text("  quiet piano ");
    ui.press("enter");
    assert_eq!(sent.get(), ["quiet piano"], "enter sends the trimmed draft");
    assert_eq!(draft.get(), "", "and clears it");
    ui.type_text("more");
    ui.click_on("composer-send")?;
    assert_eq!(sent.get().len(), 2);
    assert_eq!(
        ui.compositor_frames(1.0)?,
        0,
        "an idle composer draws nothing"
    );
    busy.set(true);
    ui.run(0.3);
    assert!(
        ui.bounds_of("composer-stop").is_some(),
        "a busy composer morphs Send into Stop"
    );
    let busy_action = ui.rect_of("composer-send")?;
    assert!(
        busy_action.size.width < idle_action.size.width
            && (busy_action.size.width - busy_action.size.height).abs() < 0.5,
        "the action morphs into a circle: idle={idle_action:?}, busy={busy_action:?}"
    );
    assert!(ui.is_animating(), "the stop ring turns while busy");
    ui.click_on("composer-input")?;
    ui.type_text("again");
    ui.press("enter");
    assert_eq!(sent.get().len(), 2, "nothing is sent while busy");
    assert_eq!(draft.get(), "again", "and the draft is kept");
    ui.click_on("composer-send")?;
    assert_eq!(stops.get(), 1, "clicking Stop fires on_stop");
    busy.set(false);
    ui.run(0.5);
    assert!(ui.bounds_of("composer-stop").is_none());
    assert_eq!(ui.rect_of("composer-send")?.size, idle_action.size);
    assert_eq!(ui.compositor_frames(1.0)?, 0, "idle again");
    ui.click_on("composer-input")?;
    ui.press("enter");
    assert_eq!(sent.get().len(), 3);
    Ok(())
}

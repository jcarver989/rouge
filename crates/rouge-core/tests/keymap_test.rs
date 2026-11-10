use rouge_core::testing::{TestResult, TestUiBuilder};
use rouge_core::{Chord, Key, Modifiers, Styled, div, text, use_binding};
use rouge_reactive::{Runtime, signal};

#[test]
fn binding_queries_follow_focus_and_ignore_shadowed_chords() -> TestResult {
    let _rt = Runtime::new().enter();
    let save = Chord::parse("cmd-s")?;
    let local = Chord::parse("cmd-p")?;
    let (mut ui, binding) = TestUiBuilder::default().build_with(|| {
        let binding = use_binding::<Save>();
        let label = text(move || binding.get().map_or(String::new(), |chord| chord.display()));
        let view = div()
            .size_full()
            .bind_key(save.clone(), Save)
            .on_action(|_: &Save| {})
            .on_action(|_: &Close| {})
            .child(
                div()
                    .test_id("local")
                    .h(25.0)
                    .focusable()
                    .bind_key(local.clone(), Save),
            )
            .child(
                div()
                    .test_id("shadow")
                    .h(25.0)
                    .focusable()
                    .bind_key(save.clone(), Close),
            )
            .child(label);
        (view, binding)
    });
    assert_eq!(ui.binding_for::<Save>(), Some(save.clone()));
    assert_eq!(binding.get(), Some(save));
    ui.click_on("local")?;
    assert_eq!(ui.binding_for::<Save>(), Some(local.clone()));
    assert_eq!(binding.get(), Some(local));
    ui.click_on("shadow")?;
    assert_eq!(ui.binding_for::<Save>(), None);
    assert_eq!(binding.get(), None);
    assert!(ui.binding_for::<Close>().is_some());
    Ok(())
}

#[test]
fn dispatch_action_uses_the_deepest_handler_without_requiring_a_binding() -> TestResult {
    let _rt = Runtime::new().enter();
    let seen = signal(0);
    let (mut ui, missing) = TestUiBuilder::default().build_with(|| {
        let binding = use_binding::<Save>();
        (
            div()
                .size_full()
                .on_action(move |_: &Save| seen.set(1))
                .child(
                    div()
                        .test_id("local")
                        .size(30.0)
                        .focusable()
                        .on_action(move |_: &Save| seen.set(2)),
                ),
            binding,
        )
    });
    assert_eq!(missing.get(), None);
    assert!(ui.dispatch_action(Save));
    assert_eq!(seen.get(), 1);
    ui.click_on("local")?;
    assert!(ui.dispatch_action(Save));
    assert_eq!(seen.get(), 2);
    assert!(!ui.dispatch_action(Close));
    Ok(())
}

#[test]
fn command_aliases_and_platform_labels_are_consistent() -> TestResult {
    let command = Chord::parse("secondary-s")?;
    assert_eq!(command, Chord::new(Key::character("s"), Modifiers::COMMAND));
    assert_eq!(command, Chord::parse("cmd-s")?);
    assert_eq!(
        command.display(),
        if cfg!(target_os = "macos") {
            "⌘S"
        } else {
            "Ctrl+S"
        }
    );
    let shifted = Chord::parse("secondary-shift-s")?;
    assert_eq!(
        shifted.display(),
        if cfg!(target_os = "macos") {
            "⇧⌘S"
        } else {
            "Ctrl+Shift+S"
        }
    );
    Ok(())
}

struct Save;
struct Close;

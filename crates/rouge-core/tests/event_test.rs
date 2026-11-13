use rouge_core::testing::TestUiBuilder;
use rouge_core::{
    Chord, ChordError, FileDropEvent, FileDropPhase, Key, KeyEvent, Modifiers, Point, Styled, div,
};
use rouge_reactive::{Runtime, signal};

#[test]
fn chords_parse_modifiers_named_keys_and_characters() -> Result<(), ChordError> {
    let save: Chord = "cmd-shift-S".parse()?;
    assert_eq!(
        save,
        Chord::new(
            Key::Character("s".into()),
            Modifiers::COMMAND.with(Modifiers::SHIFT)
        )
    );
    assert_eq!("ctrl-alt-delete".parse::<Chord>()?.key, Key::Delete);
    assert_eq!(
        "f5".parse::<Chord>()?,
        Chord::new(Key::F(5), Modifiers::NONE)
    );
    assert_eq!("esc".parse::<Chord>()?.key, Key::Escape);
    assert_eq!(
        "ctrl--".parse::<Chord>()?,
        Chord::new(Key::Character("-".into()), Modifiers::CTRL)
    );
    assert_eq!(
        "cmd-nope".parse::<Chord>(),
        Err(ChordError::UnknownKey("nope".into()))
    );
    assert_eq!("shift".parse::<Chord>(), Err(ChordError::Empty));
    Ok(())
}

#[test]
fn chords_match_events_by_key_and_modifiers() -> Result<(), ChordError> {
    let chord: Chord = "ctrl-k".parse()?;
    let key = || KeyEvent::new(Key::character("K"));
    assert!(chord.matches(&key().with_modifiers(Modifiers::CTRL)));
    assert!(
        chord.matches(&key().with_modifiers(Modifiers::CTRL.with(Modifiers::SHIFT))),
        "shift is part of a typed character"
    );
    assert!(!chord.matches(&key()));
    assert!(!chord.matches(&key().with_modifiers(Modifiers::CTRL.with(Modifiers::ALT))));
    let tab: Chord = "shift-tab".parse()?;
    assert!(
        !tab.matches(&KeyEvent::new(Key::Tab)),
        "shift matters for named keys"
    );
    assert!(tab.matches(&KeyEvent::new(Key::Tab).with_modifiers(Modifiers::SHIFT)));
    assert_eq!(Key::character(" "), Key::Space);
    Ok(())
}

#[test]
fn dropped_files_reach_the_element_under_the_pointer_or_else_the_focus_path() {
    let _rt = Runtime::new().enter();
    let (mut ui, dropped) = TestUiBuilder::default().size(200.0, 100.0).build_with(|| {
        let dropped = signal(Vec::new());
        let target = div()
            .size(100.0)
            .focusable()
            .on_file_drop(move |event: &FileDropEvent| {
                dropped.update(|all| all.push((event.phase, event.paths.clone())));
                event.stop_propagation();
            });
        (div().size_full().child(target), dropped)
    });
    ui.click(Point::new(50.0, 50.0));
    ui.pointer_left();
    assert!(ui.file_drop(FileDropPhase::Hover, vec!["unknown.png".into()]));
    ui.pointer_moved(Point::new(150.0, 50.0));
    assert!(!ui.file_drop(FileDropPhase::Drop, vec!["outside.png".into()]));
    ui.pointer_moved(Point::new(50.0, 50.0));
    assert!(ui.file_drop(FileDropPhase::Drop, vec!["photo.png".into()]));
    assert!(ui.file_drop(FileDropPhase::Leave, Vec::new()));
    assert_eq!(
        dropped.get(),
        vec![
            (FileDropPhase::Hover, vec!["unknown.png".into()]),
            (FileDropPhase::Drop, vec!["photo.png".into()]),
            (FileDropPhase::Leave, Vec::new()),
        ]
    );
}

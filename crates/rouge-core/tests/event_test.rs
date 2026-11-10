use rouge_core::{Chord, ChordError, Key, KeyEvent, Modifiers};

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

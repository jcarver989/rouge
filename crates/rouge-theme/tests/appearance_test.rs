use rouge_theme::{Appearance, AppearanceMode};
use std::error::Error;

#[test]
fn a_mode_resolves_against_the_system_appearance_only_when_it_follows_the_system() {
    for system in [Appearance::Light, Appearance::Dark] {
        assert_eq!(AppearanceMode::System.resolve(system), system);
        assert_eq!(AppearanceMode::Light.resolve(system), Appearance::Light);
        assert_eq!(AppearanceMode::Dark.resolve(system), Appearance::Dark);
    }
}

#[test]
fn appearances_serialize_in_lowercase() -> Result<(), Box<dyn Error>> {
    assert_eq!(serde_json::to_string(&Appearance::Dark)?, r#""dark""#);
    assert_eq!(
        serde_json::from_str::<AppearanceMode>(r#""system""#)?,
        AppearanceMode::System
    );
    Ok(())
}

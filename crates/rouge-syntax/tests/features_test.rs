use rouge_syntax::{SyntaxError, SyntaxStream, analyze, languages, resolve_language};

#[test]
fn enabled_languages_match_active_features() -> Result<(), SyntaxError> {
    let rust = cfg!(feature = "lang-rust");
    let sql = cfg!(feature = "lang-sql");
    for (lang, path, source, enabled) in [
        ("rust", "lib.rs", "fn main() {}\n", rust),
        ("sql", "q.sql", "SELECT 1;\n", sql),
    ] {
        assert_eq!(languages().any(|name| name == lang), enabled);
        assert_eq!(resolve_language(path, ""), enabled.then_some(lang));
        let batch = analyze(lang, source);
        let mut stream = SyntaxStream::new(lang);
        let update = stream.append(source)?;
        for highlights in [&batch.highlights, &update.highlights] {
            let spans = highlights.line(0).unwrap_or_default().iter().len();
            assert_eq!(spans > 0, enabled, "{lang}");
        }
    }
    Ok(())
}

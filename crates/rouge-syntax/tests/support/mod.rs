use rouge_syntax::{
    DocumentHighlights, HighlightLine, SyntaxError, SyntaxStream, SyntaxStreamUpdate, analyze,
};
use std::{error::Error, sync::Arc};

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) struct SyntaxBuilder {
    language: String,
}

impl Default for SyntaxBuilder {
    fn default() -> Self {
        Self {
            language: "rust".to_owned(),
        }
    }
}

impl SyntaxBuilder {
    pub(crate) fn language(mut self, language: &str) -> Self {
        language.clone_into(&mut self.language);
        self
    }

    pub(crate) fn build(self) -> SyntaxFixture {
        SyntaxFixture {
            stream: SyntaxStream::new(self.language.as_str()),
            language: self.language,
        }
    }
}

pub(crate) struct SyntaxFixture {
    pub(crate) stream: SyntaxStream,
    language: String,
}

impl SyntaxFixture {
    pub(crate) fn highlight(&self, source: &str) -> Arc<DocumentHighlights> {
        analyze(self.language.as_str(), source).highlights
    }

    pub(crate) fn append(&mut self, source: &str) -> Result<SyntaxStreamUpdate, SyntaxError> {
        self.stream.append(source)
    }

    pub(crate) fn assert_equivalent(&self) {
        let source = self.stream.source();
        let complete = analyze(self.language.as_str(), source).highlights;
        assert_eq!(self.stream.highlights().line_count(), complete.line_count());
        for index in 0..complete.line_count() {
            assert_eq!(
                self.stream.highlights().line(index),
                complete.line(index),
                "{}: {source:?}, line {index}",
                self.language
            );
        }
    }
}

pub(crate) fn line(
    highlights: &DocumentHighlights,
    index: usize,
) -> Result<HighlightLine<'_>, Box<dyn Error>> {
    highlights
        .line(index)
        .ok_or_else(|| format!("missing line {index}").into())
}

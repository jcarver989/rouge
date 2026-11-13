use crate::Rgba;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkdownPalette {
    pub heading: Rgba,
    pub link: Rgba,
    pub quote: Rgba,
    pub inline_code: Rgba,
    pub inline_code_background: Rgba,
    pub code: Rgba,
    pub code_background: Rgba,
}

impl MarkdownPalette {
    #[must_use]
    pub const fn colors(&self) -> [Rgba; 7] {
        [
            self.heading,
            self.link,
            self.quote,
            self.inline_code,
            self.inline_code_background,
            self.code,
            self.code_background,
        ]
    }
}

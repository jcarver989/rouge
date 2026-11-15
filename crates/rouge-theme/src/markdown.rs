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

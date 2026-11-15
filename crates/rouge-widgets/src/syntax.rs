use rouge_core::{Color, SpanStyle, Underline, WrapIndent};
use rouge_syntax::{HighlightLine, SyntaxTheme};
use std::ops::Range;

pub const CODE_WRAP: WrapIndent = WrapIndent::Leading { extra_columns: 2 };

#[must_use]
pub fn syntax_spans(
    spans: HighlightLine<'_>,
    theme: &SyntaxTheme,
) -> Vec<(Range<usize>, SpanStyle)> {
    let mut styled = Vec::with_capacity(spans.len());
    for span in spans.iter() {
        let Some(style) = theme.highlight(span.highlight) else {
            continue;
        };
        let foreground = Color::from(style.foreground);
        styled.push((
            span.range,
            SpanStyle {
                color: Some(foreground),
                weight: style.font_style.bold.then_some(700.0),
                italic: style.font_style.italic.then_some(true),
                underline: style.font_style.underline.then_some(Underline {
                    color: foreground,
                    thickness: 1.0,
                }),
                ..SpanStyle::default()
            },
        ));
    }
    styled
}

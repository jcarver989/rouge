use rouge_core::{Color, SpanStyle, Underline, WrapIndent};
use rouge_syntax::{HighlightLine, SyntaxTheme};
use std::ops::Range;

pub const CODE_WRAP: WrapIndent = WrapIndent::Leading { extra_columns: 2 };

#[must_use]
pub fn syntax_spans(
    line: &str,
    spans: HighlightLine<'_>,
    theme: &SyntaxTheme,
) -> Vec<(Range<usize>, SpanStyle)> {
    spans
        .iter()
        .filter_map(|span| {
            let style = theme.highlight(span.highlight)?;
            let range = span.range.start..span.range.end.min(line.len());
            let foreground = Color::from(style.foreground);
            (!range.is_empty() && line.get(range.clone()).is_some()).then_some((
                range,
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
            ))
        })
        .collect()
}

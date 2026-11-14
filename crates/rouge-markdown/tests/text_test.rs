use rouge_markdown::{
    MarkdownBlockKind, MarkdownDocument, MarkdownInline, MarkdownLinkRange, MarkdownMarkRange,
    MarkdownMarks, MarkdownText,
};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn flattens_nested_styles_into_merged_mark_ranges() -> TestResult {
    let text = paragraph_text("plain **bold _both_** ~~gone~~ `code` ![alt](image.png)")?;
    assert_eq!(text.text(), "plain bold both gone code alt");
    let bold = MarkdownMarks::STRONG;
    assert_eq!(
        text.marks(),
        [
            mark(6..11, bold),
            mark(11..15, bold.with(MarkdownMarks::EMPHASIS)),
            mark(16..20, MarkdownMarks::STRIKETHROUGH),
            mark(21..25, MarkdownMarks::CODE),
            mark(26..29, MarkdownMarks::IMAGE),
        ]
    );
    assert_eq!(text.links(), []);
    Ok(())
}

#[test]
fn soft_breaks_are_spaces_and_hard_breaks_are_newlines() -> TestResult {
    let text = paragraph_text("one\ntwo  \nthree\\\nfour")?;
    assert_eq!(text.text(), "one two\nthree\nfour");
    assert_eq!(text.marks(), []);
    Ok(())
}

#[test]
fn links_cover_their_text_and_report_whether_they_can_be_opened() -> TestResult {
    let text = paragraph_text(
        "[**docs**](https://a.dev \"Docs\") mail me@b.dev, see [local](./file.md) and https://c.dev",
    )?;
    assert_eq!(
        text.text(),
        "docs mail me@b.dev, see local and https://c.dev"
    );
    assert_eq!(
        text.links(),
        [
            link(0..4, "https://a.dev", Some("Docs")),
            link(10..18, "mailto:me@b.dev", None),
            link(24..29, "./file.md", None),
            link(34..47, "https://c.dev", None),
        ]
    );
    assert_eq!(
        text.links()
            .iter()
            .map(MarkdownLinkRange::openable)
            .collect::<Vec<_>>(),
        [true, true, false, true]
    );
    assert_eq!(
        text.marks()[0],
        mark(0..4, MarkdownMarks::STRONG.with(MarkdownMarks::LINK))
    );
    Ok(())
}

#[test]
fn link_lookup_finds_the_link_under_a_byte_offset() -> TestResult {
    let text = paragraph_text("[a](https://a.dev) gap [bc](https://b.dev)")?;
    let destination = |offset| text.link_at(offset).map(|link| link.destination.as_str());
    assert_eq!(destination(0), Some("https://a.dev"));
    assert_eq!(destination(1), None);
    assert_eq!(destination(5), None);
    assert_eq!(destination(6), Some("https://b.dev"));
    assert_eq!(destination(7), Some("https://b.dev"));
    assert_eq!(destination(8), None);
    Ok(())
}

#[test]
fn empty_content_and_empty_links_produce_no_ranges() {
    let text = MarkdownText::new(&[
        MarkdownInline::Link {
            destination: "https://a.dev".to_owned(),
            title: None,
            content: Vec::new(),
        },
        MarkdownInline::Strong(vec![MarkdownInline::Text(String::new())]),
    ]);
    assert_eq!(text, MarkdownText::default());
    assert_eq!(MarkdownText::new(&[]).text(), "");
}

#[test]
fn list_items_table_cells_and_headings_flatten_like_paragraphs() -> TestResult {
    let document =
        MarkdownDocument::parse("# Title *em*\n\n- item\n  wrapped\n\n| a **b** |\n| --- |\n");
    let [heading, list, table] = document.blocks() else {
        return Err("expected three blocks".into());
    };
    let MarkdownBlockKind::Heading { content, .. } = &heading.kind else {
        return Err("expected heading".into());
    };
    assert_eq!(MarkdownText::new(content).text(), "Title em");
    let MarkdownBlockKind::List { items, .. } = &list.kind else {
        return Err("expected list".into());
    };
    assert_eq!(MarkdownText::new(&items[0].content).text(), "item wrapped");
    let MarkdownBlockKind::Table(table) = &table.kind else {
        return Err("expected table".into());
    };
    let cell = MarkdownText::new(&table.rows[0].cells[0].content);
    assert_eq!(cell.text(), "a b");
    assert_eq!(cell.marks(), [mark(2..3, MarkdownMarks::STRONG)]);
    Ok(())
}

#[test]
fn marks_combine_and_report_containment() {
    let marks = MarkdownMarks::STRONG.with(MarkdownMarks::LINK);
    assert!(marks.contains(MarkdownMarks::STRONG));
    assert!(marks.contains(MarkdownMarks::LINK));
    assert!(!marks.contains(MarkdownMarks::CODE));
    assert!(!marks.is_empty());
    assert!(MarkdownMarks::NONE.is_empty());
    assert_eq!(MarkdownMarks::default(), MarkdownMarks::NONE);
}

fn paragraph_text(source: &str) -> Result<MarkdownText, Box<dyn Error>> {
    let document = MarkdownDocument::parse(source);
    match &document.blocks().first().ok_or("no blocks")?.kind {
        MarkdownBlockKind::Paragraph { content } => Ok(MarkdownText::new(content)),
        other => Err(format!("expected a paragraph, found {other:?}").into()),
    }
}

fn mark(range: std::ops::Range<usize>, marks: MarkdownMarks) -> MarkdownMarkRange {
    MarkdownMarkRange { range, marks }
}

fn link(
    range: std::ops::Range<usize>,
    destination: &str,
    title: Option<&str>,
) -> MarkdownLinkRange {
    MarkdownLinkRange {
        range,
        destination: destination.to_owned(),
        title: title.map(str::to_owned),
    }
}

use rouge_markdown::{
    MarkdownBlockKind, MarkdownCodeBlock, MarkdownDocument, MarkdownMarks, MarkdownOptions,
    MarkdownTargetKind, MarkdownText,
};
use rouge_syntax::{SymbolTags, analyze};
use std::{assert_matches, error::Error, ops::Range, sync::Arc};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn bare_urls_become_links_without_trailing_punctuation() -> TestResult {
    let content = paragraph("see https://example.com/a?b=1&c=2.")?;
    assert_eq!(content.text(), "see https://example.com/a?b=1&c=2.");
    assert_eq!(
        links(&content),
        [(4..33, "https://example.com/a?b=1&c=2".to_owned())]
    );
    Ok(())
}

#[test]
fn bare_urls_inside_emphasis_become_links() -> TestResult {
    let content = paragraph("*docs at http://docs.rs*")?;
    assert_eq!(links(&content), [(8..22, "http://docs.rs".to_owned())]);
    assert_eq!(
        content.marks()[1].marks,
        MarkdownMarks::EMPHASIS.with(MarkdownMarks::LINK)
    );
    Ok(())
}

#[test]
fn bare_email_addresses_become_mailto_links() -> TestResult {
    let content = paragraph("mail me@example.com")?;
    assert_eq!(
        links(&content),
        [(5..19, "mailto:me@example.com".to_owned())]
    );
    Ok(())
}

#[test]
fn explicit_links_code_and_other_schemes_are_left_alone() -> TestResult {
    let content = paragraph("[docs](https://a.dev) `https://b.dev` ftp://c.dev")?;
    assert_eq!(content.text(), "docs https://b.dev ftp://c.dev");
    assert_eq!(links(&content), [(0..4, "https://a.dev".to_owned())]);
    assert_eq!(content.marks()[1].marks, MarkdownMarks::CODE);
    Ok(())
}

#[test]
fn bare_urls_in_list_items_and_headings_become_links() {
    let document = MarkdownDocument::parse("# https://a.dev\n\n- https://b.dev\n");
    let [heading, list] = document.blocks() else {
        panic!("{:?}", document.blocks())
    };
    let MarkdownBlockKind::Heading { content, .. } = &heading.kind else {
        panic!("{heading:?}")
    };
    assert_eq!(links(content), [(0..13, "https://a.dev".to_owned())]);
    let MarkdownBlockKind::List { items, .. } = &list.kind else {
        panic!("{list:?}")
    };
    assert_eq!(
        links(&items[0].content),
        [(0..13, "https://b.dev".to_owned())]
    );
}

#[test]
fn parses_targets_in_document_order() {
    let document = MarkdownDocument::parse("# Title\n\nText\n\n```rust\nlet x = 1;\n```\n");
    assert_eq!(
        kinds(&document),
        [
            MarkdownTargetKind::Heading,
            MarkdownTargetKind::Paragraph,
            MarkdownTargetKind::CodeBlock,
            MarkdownTargetKind::CodeLine,
        ]
    );
}

#[test]
fn preserves_source_and_maps_crlf_lines() -> TestResult {
    let source = "# Héading\r\n\r\n```\r\none\r\ntwo\r\n```\r\n";
    let document = MarkdownDocument::parse(source);
    assert_eq!(document.source(), source);
    let code = code_block(&document, 1)?;
    assert_eq!(source_lines(code), [4, 5]);
    assert_eq!(line_texts(code), ["one", "two"]);
    Ok(())
}

#[test]
fn parses_nested_inline_nodes_and_table_rows() -> TestResult {
    let document = MarkdownDocument::parse(
        "**bold _em_** and ~~gone~~ [link](url) ![alt](image)\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n",
    );
    assert_matches!(
        document.blocks()[0].kind,
        MarkdownBlockKind::Paragraph { .. }
    );
    let MarkdownBlockKind::Table(table) = &document.blocks()[1].kind else {
        return Err("expected table".into());
    };
    assert_eq!(table.rows.len(), 2);
    assert_eq!(
        kinds(&document)
            .into_iter()
            .filter(|kind| *kind == MarkdownTargetKind::TableRow)
            .count(),
        2
    );
    Ok(())
}

#[test]
fn longer_outer_fence_does_not_close_on_shorter_run() -> TestResult {
    let document = MarkdownDocument::parse("````\n```\nstill code\n````\n");
    assert_eq!(line_texts(code_block(&document, 0)?), ["```", "still code"]);
    Ok(())
}

#[test]
fn fences_close_after_up_to_three_columns_of_indentation_and_trailing_spaces() -> TestResult {
    for (source, lines, after) in [
        ("```\ncode\n   ```\nafter\n", vec!["code"], true),
        ("```\ncode\n```  \r\nafter\r\n", vec!["code"], true),
        ("```\ncode\r\n```\rafter\n", vec!["code"], true),
        (
            "```\ncode\n\t```\nafter\n",
            vec!["code", "\t```", "after", ""],
            false,
        ),
        (
            "```\ncode\n \t```\nafter\n",
            vec!["code", " \t```", "after", ""],
            false,
        ),
        (
            "```\ncode\n```\t\nafter\n",
            vec!["code", "```\t", "after", ""],
            false,
        ),
        (
            "```\ncode\r```\nafter\n",
            vec!["code\r```", "after", ""],
            false,
        ),
    ] {
        let document = MarkdownDocument::parse(source);
        assert_eq!(line_texts(code_block(&document, 0)?), lines, "{source:?}");
        assert_eq!(
            document.blocks().len(),
            1 + usize::from(after),
            "{source:?}"
        );
    }
    Ok(())
}

#[test]
fn blockquote_is_the_only_target_for_its_contents() {
    let document = MarkdownDocument::parse("> # quoted\n>\n> text\n");
    assert_eq!(kinds(&document), [MarkdownTargetKind::BlockQuote]);
}

#[test]
fn nested_list_items_are_targets_without_overlapping_paragraph_targets() -> TestResult {
    let document = MarkdownDocument::parse("1. outer\n   - inner\n   - second\n2. last\n");
    assert_eq!(kinds(&document), [MarkdownTargetKind::ListItem; 4]);
    let MarkdownBlockKind::List { items, .. } = &document.blocks()[0].kind else {
        return Err("expected list".into());
    };
    assert_eq!(items[0].depth, 0);
    assert_eq!(items[0].blocks.len(), 1);
    Ok(())
}

#[test]
fn indented_code_strips_parser_indentation() -> TestResult {
    let document = MarkdownDocument::parse("    first\n    second\n");
    let code = code_block(&document, 0)?;
    assert_eq!(line_texts(code), ["first", "second"]);
    assert_eq!(code.lines()[0].source.lines.start, 1);
    assert_eq!(code.lines()[1].source.bytes, 10..20);
    Ok(())
}

#[test]
fn indented_code_preserves_blank_content_lines() -> TestResult {
    let document = MarkdownDocument::parse("    one\n\n    two\n");
    let code = code_block(&document, 0)?;
    assert_eq!(line_texts(code), ["one", "", "two"]);
    assert_eq!(source_lines(code), [1, 2, 3]);
    Ok(())
}

#[test]
fn raw_html_is_plain_fallback_without_a_target() {
    let document = MarkdownDocument::parse("<div>not executed</div>\n");
    assert_matches!(
        document.blocks()[0].kind,
        MarkdownBlockKind::HtmlFallback { .. }
    );
    assert_eq!(document.targets(), []);
}

#[test]
fn empty_and_malformed_documents_are_safe() {
    assert_eq!(MarkdownDocument::parse("").targets(), []);
    assert_eq!(MarkdownDocument::parse(""), MarkdownDocument::default());
    let document = MarkdownDocument::parse("# unclosed **emphasis\n\n```rust\ncode");
    assert_ne!(document.blocks(), []);
}

#[test]
fn soft_breaks_render_as_spaces_in_labels_and_titles() -> TestResult {
    let document = MarkdownDocument::parse("Setext\ntitle\n===\n\nfirst\nsecond  \nthird\n");
    let MarkdownBlockKind::Heading { content, .. } = &document.blocks()[0].kind else {
        return Err("expected heading".into());
    };
    assert_eq!(content.text(), "Setext title");
    let MarkdownBlockKind::Paragraph { content } = &document.blocks()[1].kind else {
        return Err("expected paragraph".into());
    };
    assert_eq!(content.text(), "first second\nthird");
    Ok(())
}

#[test]
fn intra_doc_links_drop_their_brackets_only_when_enabled() -> TestResult {
    let source = "See [`Vec`] and [Path::new], not a[b] or [two words].\n";
    let options = MarkdownOptions {
        intra_doc_links: true,
    };
    assert_eq!(
        paragraph_text(&MarkdownDocument::parse(source))?,
        "See [Vec] and [Path::new], not a[b] or [two words]."
    );
    assert_eq!(
        paragraph_text(&MarkdownDocument::parse_with(source, options))?,
        "See Vec and Path::new, not a[b] or [two words]."
    );
    Ok(())
}

#[test]
fn metadata_is_carried_without_changing_the_parse() {
    let source = "# Plan\n\nBody\n";
    let document = MarkdownDocument::parse(source)
        .with_metadata(Some("docs/plan.md".to_owned()), Some("Plan".to_owned()));
    assert_eq!(document.source_path(), Some("docs/plan.md"));
    assert_eq!(document.title(), Some("Plan"));
    assert_eq!(document.blocks(), MarkdownDocument::parse(source).blocks());
}

#[test]
fn an_unclosed_fence_at_the_end_of_a_document_is_fully_analyzed() -> TestResult {
    let document = MarkdownDocument::parse("```rust\nfn main() {}\n");
    let syntax = code_block(&document, 0)?.syntax();
    assert_ne!(**syntax.symbols(), SymbolTags::default());
    assert_eq!(
        syntax.symbols(),
        analyze("rust", "fn main() {}\n").symbols()
    );
    assert!(!syntax.highlights.line(0).ok_or("line")?.is_empty());
    Ok(())
}

fn paragraph(source: &str) -> Result<Arc<MarkdownText>, Box<dyn Error>> {
    let document = MarkdownDocument::parse(source);
    match &document.blocks().first().ok_or("no blocks")?.kind {
        MarkdownBlockKind::Paragraph { content } => Ok(Arc::clone(content)),
        other => Err(format!("expected a paragraph, found {other:?}").into()),
    }
}

fn links(content: &MarkdownText) -> Vec<(Range<usize>, String)> {
    content
        .links()
        .iter()
        .map(|link| (link.range.clone(), link.destination.clone()))
        .collect()
}

fn kinds(document: &MarkdownDocument) -> Vec<MarkdownTargetKind> {
    document
        .targets()
        .iter()
        .map(|target| target.kind)
        .collect()
}

fn code_block(
    document: &MarkdownDocument,
    index: usize,
) -> Result<&MarkdownCodeBlock, Box<dyn Error>> {
    match &document.blocks().get(index).ok_or("missing block")?.kind {
        MarkdownBlockKind::CodeBlock(code) => Ok(code),
        other => Err(format!("expected a code block, found {other:?}").into()),
    }
}

fn line_texts(code: &MarkdownCodeBlock) -> Vec<&str> {
    code.lines()
        .iter()
        .map(|line| code.line_text(line))
        .collect()
}

fn source_lines(code: &MarkdownCodeBlock) -> Vec<usize> {
    code.lines()
        .iter()
        .map(|line| line.source.lines.start)
        .collect()
}

fn paragraph_text(document: &MarkdownDocument) -> Result<String, Box<dyn Error>> {
    match &document.blocks().first().ok_or("no blocks")?.kind {
        MarkdownBlockKind::Paragraph { content } => Ok(content.text().to_owned()),
        other => Err(format!("expected a paragraph, found {other:?}").into()),
    }
}

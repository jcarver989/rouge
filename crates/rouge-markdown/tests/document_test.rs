use rouge_markdown::{MarkdownBlockKind, MarkdownDocument, MarkdownInline as Inline};

#[test]
fn bare_urls_become_links_without_trailing_punctuation() {
    assert_eq!(
        paragraph("see https://example.com/a?b=1&c=2."),
        [
            text("see "),
            link(
                "https://example.com/a?b=1&c=2",
                "https://example.com/a?b=1&c=2"
            ),
            text(".")
        ]
    );
}

#[test]
fn bare_urls_inside_emphasis_become_links() {
    assert_eq!(
        paragraph("*docs at http://docs.rs*"),
        [Inline::Emphasis(vec![
            text("docs at "),
            link("http://docs.rs", "http://docs.rs")
        ])]
    );
}

#[test]
fn bare_email_addresses_become_mailto_links() {
    assert_eq!(
        paragraph("mail me@example.com"),
        [
            text("mail "),
            link("mailto:me@example.com", "me@example.com")
        ]
    );
}

#[test]
fn explicit_links_code_and_other_schemes_are_left_alone() {
    assert_eq!(
        paragraph("[docs](https://a.dev) `https://b.dev` ftp://c.dev"),
        [
            link("https://a.dev", "docs"),
            text(" "),
            Inline::Code("https://b.dev".to_owned()),
            text(" ftp://c.dev")
        ]
    );
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
    assert_eq!(content, &[link("https://a.dev", "https://a.dev")]);
    let MarkdownBlockKind::List { items, .. } = &list.kind else {
        panic!("{list:?}")
    };
    assert_eq!(items[0].content, [link("https://b.dev", "https://b.dev")]);
}

fn paragraph(source: &str) -> Vec<Inline> {
    let document = MarkdownDocument::parse(source);
    match &document.blocks()[0].kind {
        MarkdownBlockKind::Paragraph { content } => content.clone(),
        other => panic!("expected a paragraph, found {other:?}"),
    }
}

fn text(text: &str) -> Inline {
    Inline::Text(text.to_owned())
}

fn link(destination: &str, label: &str) -> Inline {
    Inline::Link {
        destination: destination.to_owned(),
        title: None,
        content: vec![text(label)],
    }
}

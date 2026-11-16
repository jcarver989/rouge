use rouge_markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownCodeBlock, MarkdownDocument, MarkdownOptions,
    MarkdownStream,
};
use rouge_syntax::{SymbolTags, analyze};
use std::{error::Error, fmt::Write, sync::Arc};

type TestResult = Result<(), Box<dyn Error>>;

const CORPUS: &[&str] = &[
    "",
    "plain",
    "# Hello\n\n**bold** and e\u{301}界\n",
    "para one\npara one continued\n\npara two\n",
    "Streaming contract\n===\n\nsetext above\n",
    "text\n---\n\n***\n\nafter rule\n",
    "# h\npara directly after\n# h2\n",
    "- one\n- two\n\n- three\n\n\n  continuation\n\n1. a\n2. b\n\n1\n",
    "1. outer\n   - inner\n   - second\n2. last\n\n* other\n+ plus\n",
    "- [ ] task\n- [x] done\n",
    "- a\n- b\n- c\n\n- d\n",
    "1. a\n2. b\n3. c\n\n   para\n4. d\n",
    "- a [x]\n- b\n- c\n\n[x]: /u\n",
    "- a\n- b\n* c\n- d\n",
    "- a\n- b\n\n\n- c\n  - d\n  - e\nlazy\n- f\n---\n",
    "3. a\n4. b\n5. c\n> quote\n",
    "    indented\n\n    still code\n\nafter\n\n    again\n",
    "```rust\nfn main() {\n    let s = \"multi\nline\";\n}\n```\n\ndone\n",
    "```\ncode\n```",
    "```\ncode\n```\n",
    "```\ncode\n  ``` \nafter\n",
    "```\ncode\n    ```\nnot closed\n```\n\nx\n",
    "```\n```x\n```\n",
    "```\n\n```\n\nx\n",
    "```\n\r\n```\r\n\r\nx\r\n",
    "````\n```\nstill code\n````\n\nout\n",
    "~~~text\nthis ~~ fence holds ``` backtick ``` lines inside\n~~~\n\n```js\nlet first = 1;\n```\n```text\nsecond\n```\n",
    "```a`b\nnot a fence\n```\n",
    "   ```rust\n   indented fence\n   ```\n\ntext\n",
    "\t```\ntab fence is code\n",
    "para\n```rust\ninterrupts\n```\nback\n",
    "```rust\nfn unclosed(",
    "```rust\nfn main() {}\n```\rfoo\n",
    "```rust\ncode\n```\r",
    "```rust\ncode\n```  ",
    "```rust\ncode\n```\t\nmore\n```\n",
    "> quote\n> more\n\n> second\n\nlazy\n> q\ncontinued\n",
    "> # quoted\n>\n> ```rust\n> code\n> ```\n> text\n",
    "- item\n\n  ```rust\n  nested\n  ```\n\n- next\n",
    "| a | b |\n| --- | :-: |\n| 1 | 2 |\n\nx\n",
    "| a |\n|---|\n| 1 |\nno pipe\n\n| c |\n",
    "<div>\nhtml\n\nafter\n",
    "<pre>\nx\n\ny\n</pre>\n\nz\n",
    "<!-- comment\n\nstill -->\ntext\n",
    "[link][later]\n\n[later]: /target\n",
    "[a]: /u 'ti'\n\n[a] and [A]\n",
    "[a]: /u\n[a]: /v\n\n[A]\n\n[a]: /w\n",
    "> [b]: /q\n\n[b]\n",
    "[c]:\n/u\n\n[c]\n",
    "[d]: /u 'unterminated\n\n[d]\n",
    "[e] early\n\ntext\n\n[e]: /late\n\n![img][e]\n",
    "use [x] here\n\n```\n[x]: /not-a-def\n```\n\n[x]: /real\n",
    "text with [brackets] but no defs\n\nmore\n",
    "[link](url) ![alt](image) `code` ~~gone~~ _em_ **strong**\n",
    "a\tb\n\n  leading spaces\n\ntrailing spaces  \nhard break\\\nnext\n",
    "\n\n\n",
    "   \n\t\n",
    "\u{a0}nbsp is not blank\n\u{a0}\nstill paragraph\n",
    "x\n\x0c\nform feed blank\n",
    "line\r\nwindows\r\n\r\n```py\r\nprint('x')\r\n```\r\n\r\nend\r\n",
    "# unclosed **emphasis\n\n```rust\ncode",
    "1) paren\n2) list\n\n2. dot\n",
    "- a\n\n\n- b\n",
    "-\n- empty item\n",
    "* * *\n- - -\n",
    "Foo\n===\nBar\n",
    "Foo\n= = =\n",
    "term\n: not definition list\n",
    "&amp; entities &copy; and \\*escapes\\*\n",
    "auto <https://example.com> link\n",
    "***bold italic*** and ___both___\n",
    "line one  \nline two\n",
    "> - quoted list\n> - two\n>\n> para\n",
    "1. a\n\n   b\n\n   ```\n   c\n   ```\n2. d\n",
    "```\n```\n```\n```\n",
    "```\ncode\n \t```\nafter\n",
    "```\ncode\n``` \t\nafter\n",
    "```\ncode\r```\nafter\n",
    "```\ncode\n```  \r\nafter\r\n",
    "```\r\ncode\r\n\t```\r\n   ```\r\nx\r\n",
    "~~~\ncode\n\t~~~\n   ~~~ \nx\n",
    "```\n``\r\n```\r",
    "```\n  `` \n```\n",
];

#[test]
fn every_prefix_of_every_source_matches_the_one_shot_parse() -> TestResult {
    for source in CORPUS {
        for chunk_chars in [1, 3, 8, 64] {
            let mut stream = MarkdownStream::new();
            let mut characters = source.chars();
            loop {
                let chunk: String = characters.by_ref().take(chunk_chars).collect();
                if chunk.is_empty() {
                    break;
                }
                stream.push(&chunk);
                assert_matches_one_shot(&stream)?;
            }
            stream.finish();
            assert_matches_one_shot(&stream)?;
            assert_eq!(stream.source(), *source);
        }
    }
    Ok(())
}

#[test]
fn concatenated_corpus_documents_stream_correctly() -> TestResult {
    let mut stream = MarkdownStream::new();
    for source in CORPUS {
        for chunk in source.as_bytes().chunks(5) {
            stream.push(std::str::from_utf8(chunk)?);
            assert_matches_one_shot(&stream)?;
        }
        stream.push("\n\n");
        assert_matches_one_shot(&stream)?;
    }
    assert!(stream.settled_blocks() > 50, "{}", stream.settled_blocks());
    Ok(())
}

#[test]
fn aether_exactness_message_matches_at_every_chunk() -> TestResult {
    let message = exactness_message();
    for chunk_bytes in [7, 37, 128, 1024] {
        let mut stream = MarkdownStream::new();
        for chunk in chunk_message(&message, chunk_bytes) {
            stream.push(&chunk);
            assert_matches_one_shot(&stream)?;
        }
        stream.finish();
        assert_matches_one_shot(&stream)?;
    }
    Ok(())
}

#[test]
fn generated_documents_match_the_one_shot_parse() -> TestResult {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..3000 {
        let source = generate(&mut seed);
        let mut stream = MarkdownStream::new();
        for chunk in chunk_message(&source, 1 + (seed % 5) as usize) {
            stream.push(&chunk);
            assert_matches_one_shot(&stream)?;
        }
    }
    Ok(())
}

#[test]
fn streaming_prose_has_bounded_parser_work() -> TestResult {
    let source = prose_message(16 * 1024);
    let mut stream = MarkdownStream::new();
    for chunk in chunk_message(&source, 256) {
        stream.push(&chunk);
    }
    stream.finish();
    let stats = stream.parse_stats();
    let budget = 4 * source.len() + 64 * 1024;
    assert!(stats.parsed_bytes <= budget, "{stats:?}, budget {budget}");
    assert!(stats.scanned_bytes <= budget, "{stats:?}, budget {budget}");
    assert_eq!(stats.unsettled_blocks, 0);
    assert!(stream.settled_blocks() + 1 >= stream.document().blocks().len());
    assert_matches_one_shot(&stream)
}

#[test]
fn streaming_code_fence_has_bounded_parser_work() -> TestResult {
    let source = code_block_message(24 * 1024);
    let mut stream = MarkdownStream::new();
    let mut parses_while_open = 0;
    for chunk in chunk_message(&source, 256) {
        let before = stream.parse_stats().parses;
        stream.push(&chunk);
        if stream.open_code_block().is_some() && before > 0 {
            parses_while_open += stream.parse_stats().parses - before;
        }
    }
    stream.finish();
    let stats = stream.parse_stats();
    let budget = 4 * source.len() + 64 * 1024;
    assert!(stats.parsed_bytes <= budget, "{stats:?}, budget {budget}");
    assert!(stats.scanned_bytes <= budget, "{stats:?}, budget {budget}");
    assert_eq!(parses_while_open, 0);
    assert!(stats.parses <= 4, "{stats:?}");
    assert!(stream.open_code_block().is_none());
    assert_eq!(stream.settled_blocks(), 1);
    assert_matches_one_shot(&stream)
}

#[test]
fn tiny_chunks_scale_linearly_with_input_size() -> TestResult {
    let mut previous = None;
    for size in [8 * 1024, 16 * 1024, 32 * 1024] {
        let mut work = 0;
        for source in [prose_message(size), code_block_message(size)] {
            let mut stream = MarkdownStream::new();
            for chunk in chunk_message(&source, 3) {
                stream.push(&chunk);
            }
            let stats = stream.parse_stats();
            work += stats.parsed_bytes + stats.scanned_bytes;
            assert_matches_one_shot(&stream)?;
        }
        if let Some(previous) = previous {
            assert!(
                work <= previous * 5 / 2,
                "doubling input {size} grew work from {previous} to {work}"
            );
        }
        previous = Some(work);
    }
    Ok(())
}

#[test]
fn a_streamed_list_reparses_only_its_open_item() -> TestResult {
    let mut previous = None;
    for items in [250, 500, 1000] {
        let source = list_message(items);
        let mut stream = MarkdownStream::new();
        for chunk in chunk_message(&source, 8) {
            stream.push(&chunk);
        }
        stream.finish();
        let work = stream.parse_stats().parsed_bytes;
        if let Some(previous) = previous {
            assert!(
                work <= previous * 5 / 2,
                "doubling a list to {items} items grew parse work from {previous} to {work}"
            );
        }
        previous = Some(work);
        assert_matches_one_shot(&stream)?;
    }
    Ok(())
}

#[test]
fn open_fences_report_their_block_and_close_precisely() -> TestResult {
    let mut stream = MarkdownStream::new();
    stream.push("intro\n\n```rust\n");
    assert_eq!(stream.open_code_block(), Some(1));
    assert_eq!(stream.settled_blocks(), 1);
    let parses = stream.parse_stats().parses;
    stream.push("let a = 1;\n``");
    assert_eq!(stream.open_code_block(), Some(1));
    assert_eq!(stream.parse_stats().parses, parses);
    stream.push("`");
    assert_matches_one_shot(&stream)?;
    assert_eq!(stream.open_code_block(), Some(1));
    stream.push("x\n");
    assert_matches_one_shot(&stream)?;
    assert_eq!(stream.open_code_block(), Some(1));
    stream.push("```\nafter\n");
    assert_matches_one_shot(&stream)?;
    assert_eq!(stream.open_code_block(), None);
    assert_eq!(stream.settled_blocks(), 2);
    let MarkdownBlockKind::CodeBlock(code) = &stream.document().blocks()[1].kind else {
        return Err("expected code block".into());
    };
    assert_eq!(
        code.lines()
            .iter()
            .map(|line| code.line_text(line))
            .collect::<Vec<_>>(),
        ["let a = 1;", "```x"]
    );
    Ok(())
}

#[test]
fn late_reference_definitions_reparse_only_bracketed_blocks() -> TestResult {
    let mut stream = MarkdownStream::new();
    stream.push("plain paragraph\n\nsee [docs] now\n\nanother\n\n");
    assert_eq!(stream.settled_blocks(), 3);
    let revision = stream.revision();
    let update = stream.push("[docs]: https://example.com\n");
    assert_eq!(update.changed_blocks.start, 1);
    assert_eq!(stream.changes_since(revision).first_block, 1);
    assert_eq!(stream.parse_stats().unsettled_blocks, 1);
    assert_matches_one_shot(&stream)?;
    let MarkdownBlockKind::Paragraph { content } = &stream.document().blocks()[1].kind else {
        return Err("expected paragraph".into());
    };
    assert!(format!("{content:?}").contains("Link"));
    Ok(())
}

#[test]
fn change_tracking_reports_the_earliest_block_since_a_revision() -> TestResult {
    let mut stream = MarkdownStream::new();
    let first = stream.push("one\n\n");
    assert_eq!(first.changed_blocks, 0..1);
    let second = stream.push("two\n\n");
    assert_eq!(second.changed_blocks, 1..2);
    let third = stream.push("three");
    assert_eq!(third.changed_blocks, 2..3);
    assert_eq!(stream.changes_since(first.revision).first_block, 1);
    assert_eq!(stream.changes_since(second.revision).first_block, 2);
    assert_eq!(stream.changes_since(third.revision).first_block, 3);
    assert!(!stream.changes_since(0).replaced);
    assert_eq!(stream.changes_since(0).first_block, 0);
    let finished = stream.finish();
    assert_eq!(finished.changed_blocks, 3..3);
    assert_eq!(stream.changes_since(third.revision).first_block, 3);
    let replaced = stream.replace("new");
    assert_eq!(replaced.changed_blocks, 0..1);
    assert!(stream.changes_since(finished.revision).replaced);
    assert!(!stream.changes_since(replaced.revision).replaced);
    assert_eq!(stream.changes_since(replaced.revision).first_block, 1);
    assert_matches_one_shot(&stream)?;
    stream.push("\n\n```\nopen");
    assert_matches_one_shot(&stream)?;
    assert_eq!(stream.open_code_block(), Some(1));
    Ok(())
}

#[test]
fn cloned_streams_parse_independently() -> TestResult {
    let mut stream = MarkdownStream::new();
    stream.push("```rust\nshared\n");
    let mut cloned = stream.clone();
    cloned.push("```\n\nclosed\n");
    stream.push("still open\n");
    assert_matches_one_shot(&stream)?;
    assert_matches_one_shot(&cloned)?;
    assert_eq!(stream.open_code_block(), Some(0));
    assert_eq!(cloned.open_code_block(), None);
    Ok(())
}

#[test]
fn growing_structural_tails_do_not_reprocess_settled_prefixes() -> TestResult {
    for tail in [
        "ordinary words ".repeat(128),
        "- item with **emphasis**\n".repeat(96),
        "> quoted words\n".repeat(96),
        format!("- parent\n\n  ```rust\n{}", "  let value = 1;\n".repeat(96)),
    ] {
        let mut stream = MarkdownStream::new();
        stream.push(&"# settled heading\n\n".repeat(256));
        let before = stream.parse_stats();
        let prefix_blocks = stream.document().blocks().len();
        let mut tail_bytes = 0;
        let mut expected_parse_work = 0;
        for chunk in chunk_message(&tail, 17) {
            tail_bytes += chunk.len();
            expected_parse_work += tail_bytes + 2;
            stream.push(&chunk);
            assert_matches_one_shot(&stream)?;
        }
        let after = stream.parse_stats();
        assert_eq!(stream.settled_blocks(), prefix_blocks);
        assert!(after.parsed_bytes - before.parsed_bytes <= expected_parse_work);
        assert_eq!(
            after.source_bytes_copied - before.source_bytes_copied,
            tail.len()
        );
        assert!(after.parsed_bytes - before.parsed_bytes >= tail.len());
    }
    Ok(())
}

#[test]
fn settled_reference_definitions_resolve_without_copying_source() -> TestResult {
    let mut stream = MarkdownStream::new();
    stream.push("[docs]: /destination\n\n# settled\n\n");
    let before = stream.parse_stats();
    stream.push("use [docs]");
    let after = stream.parse_stats();
    assert_eq!(
        after.source_bytes_copied - before.source_bytes_copied,
        "use [docs]".len()
    );
    assert!(after.parsed_bytes - before.parsed_bytes <= "\nuse [docs]".len());
    assert_matches_one_shot(&stream)
}

#[test]
fn settled_definitions_win_over_later_duplicates_in_the_tail() -> TestResult {
    let mut stream = MarkdownStream::new();
    stream.push("[a]: /first\n\n# settled\n\n- [a]\n\n");
    stream.push("[a]: /second\n");
    assert_matches_one_shot(&stream)
}

#[test]
fn intra_doc_links_stream_like_the_one_shot_parse() -> TestResult {
    let options = MarkdownOptions {
        intra_doc_links: true,
    };
    for source in [
        "Uses [`Vec`] and [Path::new].\n\nThen [later] and a[b].\n\n[later]: https://example.com\n",
        "[`Option`]\n\n```rust\nlet [a] = [1];\n```\n\nsee [crate::x]\n",
    ] {
        for chunk_chars in [1, 4, 64] {
            let mut stream = MarkdownStream::with_options(options);
            let characters = source.chars().collect::<Vec<_>>();
            for chunk in characters.chunks(chunk_chars) {
                stream.push(&chunk.iter().collect::<String>());
                assert_eq!(
                    stream.document(),
                    &MarkdownDocument::parse_with(stream.source(), options),
                    "{:?}",
                    stream.source()
                );
            }
        }
    }
    Ok(())
}

#[test]
fn source_updates_report_byte_ranges_without_promising_stable_prefixes() {
    let mut stream = MarkdownStream::new();
    let first = stream.push("hello\n\n");
    assert_eq!(first.revision, 1);
    assert_eq!(first.changed_source, 0..7);
    let second = stream.push("世界");
    assert_eq!(second.revision, 2);
    assert_eq!(second.changed_source, 7..13);
    assert_eq!(stream.source(), "hello\n\n世界");
    let finished = stream.finish();
    assert_eq!(finished.revision, 3);
    assert_eq!(finished.changed_source, 13..13);
    assert!(finished.finished);
    assert!(stream.is_finished());
}

#[test]
fn operations_that_change_nothing_keep_the_revision() {
    let mut stream = MarkdownStream::new();
    stream.push("text");
    let revision = stream.revision();
    let empty = stream.push("");
    assert_eq!(empty.revision, revision);
    assert!(empty.changed_source.is_empty());
    assert!(!empty.reset);
    stream.finish();
    let again = stream.finish();
    assert_eq!(again.revision, revision + 1);
    assert!(again.finished);
    assert_eq!(stream.revision(), revision + 1);
    let resumed = stream.push("");
    assert_eq!(resumed.revision, revision + 2);
    assert!(resumed.reset);
    assert!(!stream.is_finished());
}

#[test]
fn replacement_and_appending_after_completion_report_resets() {
    let mut stream = MarkdownStream::new();
    stream.push("```rust\nopen\n");
    let identity = stream.identity();
    let replaced = stream.replace("x");
    assert!(replaced.reset);
    assert_eq!(replaced.changed_source, 0..1);
    assert_eq!(stream.source(), "x");
    assert_eq!(stream.identity(), identity);
    stream.finish();
    let resumed = stream.push("é");
    assert!(resumed.reset);
    assert!(!resumed.finished);
    assert_eq!(resumed.changed_source, 1..3);
    assert_eq!(stream.source(), "xé");
}

#[test]
fn source_revisions_only_advance_when_source_can_change() {
    let mut stream = MarkdownStream::new();
    assert_eq!(stream.source_revision(), 0);
    stream.push("café");
    let source_revision = stream.source_revision();
    assert_eq!(source_revision, 1);
    stream.push("");
    stream.finish();
    stream.finish();
    stream.push("");
    assert_eq!(stream.source_revision(), source_revision);
    assert_eq!(stream.clone().source_revision(), source_revision);
    stream.replace("café");
    assert_eq!(stream.source_revision(), source_revision + 1);
    stream.push("!");
    assert_eq!(stream.source_revision(), source_revision + 2);
}

#[test]
fn independent_and_cloned_streams_have_distinct_identities() {
    let mut first = MarkdownStream::new();
    first.push("before");
    let mut cloned = first.clone();
    let second = MarkdownStream::new();
    assert_ne!(first.identity(), second.identity());
    assert_ne!(first.identity(), cloned.identity());
    assert_eq!(first.revision(), cloned.revision());
    assert_eq!(first.source(), cloned.source());
    first.push(" first");
    cloned.push(" second");
    assert_eq!(first.revision(), cloned.revision());
    assert_ne!(first.source(), cloned.source());
}

#[test]
fn an_open_code_block_is_highlighted_as_it_streams_and_keeps_that_analysis_when_closed()
-> Result<(), Box<dyn Error>> {
    let code = "fn main() {\n    let a = 1;\n}";
    let mut stream = MarkdownStream::new();
    stream.push("```rust\nfn main");
    stream.push(&code["fn main".len()..]);
    assert_eq!(stream.open_code_block(), Some(0));
    let expected = analyze("rust", code).highlights;
    let streamed = Arc::clone(&first_code(&stream)?.syntax().highlights);
    for line in 0..3 {
        assert_eq!(streamed.line(line), expected.line(line));
    }
    let highlighted = stream.parse_stats().highlighted_bytes;
    stream.push("\n``");
    stream.push("`\n");
    assert_eq!(stream.open_code_block(), None);
    let closed = &first_code(&stream)?.syntax().highlights;
    assert!(
        Arc::ptr_eq(closed, &streamed),
        "closing reuses the streamed analysis"
    );
    assert_eq!(stream.parse_stats().highlighted_bytes, highlighted);
    assert_eq!(closed.line_count(), 3);
    assert_eq!(first_code(&stream)?.text(), code);
    assert_eq!(
        closed.line(2).ok_or("line")?,
        expected.line(2).ok_or("line")?
    );
    Ok(())
}

#[test]
fn changes_report_the_first_changed_line_of_the_open_code_block() {
    let mut stream = MarkdownStream::new();
    stream.push("Intro\n\n```rust\nfn a() {}\n");
    let revision = stream.revision();
    stream.push("fn b() {}\n");
    let changes = stream.changes_since(revision);
    assert_eq!((changes.first_block, changes.first_line), (1, 1));
    stream.push("```\n\nAfter\n");
    assert_eq!(stream.changes_since(revision).first_block, 1);
}

#[test]
fn finishing_an_open_code_block_completes_its_analysis_and_reports_the_change()
-> Result<(), Box<dyn Error>> {
    let mut stream = MarkdownStream::new();
    stream.push("Intro\n\n```rust\nfn main() {}\n``");
    assert_eq!(stream.open_code_block(), Some(1));
    let open = first_code(&stream)?.syntax().clone();
    assert_eq!(**open.symbols(), SymbolTags::default());
    let revision = stream.revision();
    let finished = stream.finish();
    assert_eq!(finished.changed_blocks, 1..2);
    assert_eq!(stream.changes_since(revision).first_block, 1);
    let code = first_code(&stream)?;
    assert_eq!(code.text(), "fn main() {}\n``");
    let expected = analyze("rust", code.text());
    assert_eq!(code.syntax().symbols(), expected.symbols());
    assert_eq!(
        code.syntax().highlights.line(1).ok_or("line")?,
        expected.highlights.line(1).ok_or("line")?
    );
    stream.push("`\n\nAfter\n");
    assert_eq!(stream.open_code_block(), None);
    assert_eq!(first_code(&stream)?.text(), "fn main() {}");
    assert_eq!(
        first_code(&stream)?.syntax().symbols(),
        analyze("rust", "fn main() {}").symbols()
    );
    Ok(())
}

#[test]
fn finishing_without_new_highlights_reports_no_change_and_does_no_parsing()
-> Result<(), Box<dyn Error>> {
    let mut stream = MarkdownStream::new();
    stream.push("```rust\nfn main() {}\n```");
    assert_eq!(stream.open_code_block(), Some(0));
    let revision = stream.revision();
    let streamed = Arc::clone(&first_code(&stream)?.syntax().highlights);
    let highlighted = stream.parse_stats().highlighted_bytes;
    let finished = stream.finish();
    assert_eq!(finished.changed_blocks, 1..1);
    assert_eq!(stream.changes_since(revision).first_block, 1);
    assert_eq!(
        first_code(&stream)?.syntax().symbols(),
        analyze("rust", "fn main() {}").symbols()
    );
    assert!(Arc::ptr_eq(
        &first_code(&stream)?.syntax().highlights,
        &streamed
    ));
    assert_eq!(stream.parse_stats().highlighted_bytes, highlighted);
    Ok(())
}

#[test]
fn unchanged_blocks_before_the_reported_change_can_be_reused() {
    let mut stream = MarkdownStream::new();
    stream.push("# Title\n\nfirst\n\n- a\n");
    let revision = stream.revision();
    let before = stream.document().blocks().to_vec();
    stream.push("- b\n\nlast");
    let changes = stream.changes_since(revision);
    assert!(!changes.replaced);
    assert_eq!(changes.first_block, 2);
    assert_eq!(&stream.document().blocks()[..2], &before[..2]);
    assert_ne!(stream.document().blocks()[2], before[2]);
}

fn assert_matches_one_shot(stream: &MarkdownStream) -> TestResult {
    let expected = MarkdownDocument::parse(stream.source());
    let actual = stream.document();
    if actual != &expected {
        let blocks = expected.blocks().len();
        for (index, (left, right)) in actual
            .blocks()
            .iter()
            .zip(expected.blocks().iter())
            .enumerate()
        {
            if left != right {
                return Err(format!(
                    "block {index} of {blocks} differs for source {:?}\n incremental: {left:#?}\n one-shot: {right:#?}",
                    stream.source()
                )
                .into());
            }
        }
        return Err(format!(
            "document differs for source {:?}\n incremental: blocks {}, targets {:?}, styles {:?}\n one-shot: blocks {}, targets {:?}, styles {:?}",
            stream.source(),
            actual.blocks().len(),
            actual.targets(),
            actual.source_styles(),
            expected.blocks().len(),
            expected.targets(),
            expected.source_styles(),
        )
        .into());
    }
    Ok(())
}

fn exactness_message() -> String {
    let mut message = String::from("Streaming contract\n===\n\n");
    message.push_str(
        "The paragraph above is a setext heading, so no line of it may be finalized early.\n\n",
    );
    message.push_str("```rust\nfn one() {\n    let s = \"starts here\n");
    for index in 0..40 {
        let _ = writeln!(message, "and continues {index}");
    }
    message.push_str("    ```\n");
    message.push_str("ends here\";\n\nlet two = one();\n}\n```\n\n");
    message.push_str("Between blocks.\n\n```python\ndef f():\n    doc = \"\"\"First line\n");
    for index in 0..40 {
        let _ = writeln!(message, "doc line {index}");
    }
    message.push_str("last line\"\"\"\n    return doc\n```\n\n");
    message.push_str("~~~text\nthis ~~ fence holds ``` backtick ``` lines inside\n~~~\n\n");
    message.push_str("```js\nlet first = 1;\n```\n```text\nsecond\n```\n\n");
    message.push_str("Tail paragraph before the still-open block.\n\n```rust\nfn unclosed(");
    message.push_str(&"x".repeat(600));
    message
}

fn prose_message(total_bytes: usize) -> String {
    let mut message = String::new();
    let mut sentence = 0;
    while message.len() < total_bytes {
        for _ in 0..4 {
            let _ = write!(
                message,
                "Sentence {sentence} carries ordinary words so wrapping and parsing do real work. "
            );
            sentence += 1;
        }
        message.push_str("\n\n");
    }
    message
}

fn code_block_message(total_bytes: usize) -> String {
    let mut message = String::from("```rust\n");
    let mut line = 0;
    while message.len() < total_bytes {
        let _ = writeln!(
            message,
            "let value_{line} = state.reconcile(incoming[{line}]).expect(\"delta accepted\");"
        );
        line += 1;
    }
    message.push_str("```\n");
    message
}

fn list_message(items: usize) -> String {
    let mut message = String::from("Intro\n\n");
    for item in 0..items {
        let _ = writeln!(
            message,
            "{}. item {item} with [a link](/x) and *some* words",
            item + 1
        );
    }
    message
}

fn chunk_message(message: &str, chunk_bytes: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut rest = message;
    while !rest.is_empty() {
        let end = rest.ceil_char_boundary(rest.len().min(chunk_bytes));
        chunks.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    chunks
}

fn generate(seed: &mut u64) -> String {
    const PIECES: &[&str] = &[
        "para ",
        "[x]",
        "[x]: /u\n",
        "[y]: /v 't'\n",
        "\n",
        "\n\n",
        "# h\n",
        "===\n",
        "---\n",
        "- item\n",
        "  cont\n",
        "1. one\n",
        "> q\n",
        "```\n",
        "```rust\n",
        "~~~\n",
        "    code\n",
        "| a | b |\n",
        "|---|---|\n",
        "<div>\n",
        "</div>\n",
        "<pre>\n",
        "`tick`",
        "**b**",
        "\r\n",
        "\t",
        "  ",
        "  ```\n",
        "    ```\n",
        "``` \n",
        "e\u{301}界",
        "![i][x]",
        "\t```\n",
        "```\r",
        "``` \t\n",
    ];
    let mut next = || {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    };
    let count = 1 + (next() % 24) as usize;
    (0..count)
        .map(|_| {
            PIECES[usize::try_from(next() % PIECES.len() as u64)
                .expect("index is below PIECES.len()")]
        })
        .collect()
}

fn first_code(stream: &MarkdownStream) -> Result<&MarkdownCodeBlock, Box<dyn Error>> {
    let blocks = stream.document().blocks().iter();
    let mut code = blocks.flat_map(MarkdownBlock::code_blocks);
    Ok(code.next().ok_or("no code block")?)
}

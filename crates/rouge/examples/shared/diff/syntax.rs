use std::rc::Rc;

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];
const CONFIG_KEYWORDS: &[&str] = &["true", "false", "null"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Token {
    Plain,
    Keyword,
    Type,
    Function,
    String,
    Number,
    Comment,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Span {
    pub text: String,
    pub token: Token,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Language {
    Rust,
    Config,
    Plain,
}

impl Language {
    pub(super) fn from_path(path: &str) -> Language {
        match path.rsplit_once('.').map(|(_, extension)| extension) {
            Some("rs") => Language::Rust,
            Some("toml" | "yaml" | "yml" | "sh" | "py") => Language::Config,
            _ => Language::Plain,
        }
    }

    fn comment(self) -> &'static str {
        if self == Language::Rust { "//" } else { "#" }
    }

    fn keywords(self) -> &'static [&'static str] {
        if self == Language::Rust {
            RUST_KEYWORDS
        } else {
            CONFIG_KEYWORDS
        }
    }
}

pub(super) fn highlight(line: &str, language: Language) -> Rc<[Span]> {
    if language == Language::Plain {
        return Rc::new([Span {
            text: line.to_owned(),
            token: Token::Plain,
        }]);
    }
    let mut lexer = Lexer {
        rest: line,
        gap: "",
        language,
        spans: Vec::new(),
    };
    lexer.run();
    lexer.spans.into()
}

struct Lexer<'a> {
    rest: &'a str,
    gap: &'a str,
    language: Language,
    spans: Vec<Span>,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) {
        loop {
            self.gap = self.take_while(char::is_whitespace);
            let Some(first) = self.rest.chars().next() else {
                break;
            };
            if self.rest.starts_with(self.language.comment()) {
                self.push(self.rest.len(), Token::Comment);
            } else if first == '"' {
                self.push(string_len(self.rest), Token::String);
            } else if let Some(len) =
                char_len(self.rest).filter(|_| self.language == Language::Rust)
            {
                self.push(len, Token::String);
            } else if first.is_ascii_digit() {
                self.push(number_len(self.rest), Token::Number);
            } else if is_word(first) {
                let len = self.rest.find(|c| !is_word(c)).unwrap_or(self.rest.len());
                let token = self.classify(&self.rest[..len], &self.rest[len..]);
                self.push(len, token);
            } else {
                self.push(first.len_utf8(), Token::Plain);
            }
        }
    }

    fn classify(&self, word: &str, after: &str) -> Token {
        if self.language.keywords().contains(&word) {
            Token::Keyword
        } else if after.starts_with('(') || after.starts_with("!(") || after.starts_with("![") {
            Token::Function
        } else if word.starts_with(char::is_uppercase) {
            Token::Type
        } else {
            Token::Plain
        }
    }

    fn take(&mut self, len: usize) -> &'a str {
        let (head, tail) = self.rest.split_at(len);
        self.rest = tail;
        head
    }

    fn take_while(&mut self, matches: impl Fn(char) -> bool) -> &'a str {
        let len = self.rest.find(|c| !matches(c)).unwrap_or(self.rest.len());
        self.take(len)
    }

    fn push(&mut self, len: usize, token: Token) {
        let text = self.take(len);
        match self.spans.last_mut() {
            Some(last) if last.token == token => {
                last.text.push_str(self.gap);
                last.text.push_str(text);
            }
            _ => self.spans.push(Span {
                text: format!("{}{text}", self.gap),
                token,
            }),
        }
        self.gap = "";
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn string_len(text: &str) -> usize {
    let mut escaped = false;
    for (index, c) in text.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return index + 1,
            _ => {}
        }
    }
    text.len()
}

fn char_len(text: &str) -> Option<usize> {
    let mut chars = text.strip_prefix('\'')?.char_indices();
    let end = match chars.next()? {
        (_, '\\') => chars.skip(1).find(|(_, c)| *c == '\'')?.0,
        _ => chars.next().filter(|(_, c)| *c == '\'')?.0,
    };
    Some(end + 2)
}

fn number_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let part_of_number = |index: usize| {
        let fraction = bytes[index] == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit);
        bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_' || fraction
    };
    (0..bytes.len())
        .find(|index| !part_of_number(*index))
        .unwrap_or(bytes.len())
}

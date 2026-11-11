use proc_macro2::{TokenStream, TokenTree};
use quote::{ToTokens, TokenStreamExt, quote};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::{Pair, Punctuated};
use syn::token::{Brace, Bracket, Paren};
use syn::{Block, Expr, ExprBlock, Ident, Lit, LitStr, Path, Stmt, Token, braced, parenthesized};

const EXPRESSION_KEYWORDS: [&str; 12] = [
    "move", "if", "match", "loop", "while", "for", "async", "unsafe", "return", "let", "static",
    "const",
];

pub(crate) struct Nodes(pub(crate) Vec<Node>);

pub(crate) enum Node {
    Element(Element),
    Text(LitStr),
    Expr(Expr),
    Spread(Expr),
}

pub(crate) struct Element {
    pub(crate) name: Path,
    pub(crate) attrs: Vec<Attr>,
    pub(crate) children: Vec<Node>,
}

pub(crate) enum Attr {
    Flag(Ident),
    Value(Ident, Value),
    Variant(Ident, Ident, Option<Value>),
    Maybe(Ident, Value),
    Let(Ident),
}

pub(crate) enum Value {
    Expr(Expr),
    Args(Punctuated<Expr, Token![,]>),
}

impl Parse for Nodes {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut nodes = Vec::new();
        while !input.is_empty() {
            nodes.push(input.parse()?);
        }
        Ok(Nodes(nodes))
    }
}

impl Parse for Node {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(Token![<]) {
            return input.parse().map(Node::Element);
        }
        if input.peek(LitStr) {
            return input.parse().map(Node::Text);
        }
        if input.peek(Brace) {
            let content;
            let brace = braced!(content in input);
            if content.peek(Token![..]) {
                content.parse::<Token![..]>()?;
                return content.parse().map(Node::Spread);
            }
            return block_expr(brace, &content).map(Node::Expr);
        }
        Err(input.error("expected an element `<name …>`, a string literal or a `{…}` block"))
    }
}

impl Parse for Element {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Token![<]>()?;
        let name = Path::parse_mod_style(input)?;
        let mut attrs = Vec::new();
        loop {
            if input.peek(Token![/]) {
                input.parse::<Token![/]>()?;
                input.parse::<Token![>]>()?;
                return Ok(Element {
                    name,
                    attrs,
                    children: Vec::new(),
                });
            }
            if input.peek(Token![>]) {
                input.parse::<Token![>]>()?;
                break;
            }
            if input.is_empty() {
                return Err(syn::Error::new_spanned(&name, "unterminated tag"));
            }
            attrs.push(input.parse()?);
        }
        let mut children = Vec::new();
        loop {
            if input.peek(Token![<]) && input.peek2(Token![/]) {
                input.parse::<Token![<]>()?;
                input.parse::<Token![/]>()?;
                let close = Path::parse_mod_style(input)?;
                if close.to_token_stream().to_string() != name.to_token_stream().to_string() {
                    let expected = name.to_token_stream();
                    return Err(syn::Error::new_spanned(
                        close,
                        format!("expected `</{expected}>`"),
                    ));
                }
                input.parse::<Token![>]>()?;
                return Ok(Element {
                    name,
                    attrs,
                    children,
                });
            }
            if input.is_empty() {
                let open = name.to_token_stream();
                return Err(syn::Error::new_spanned(
                    &name,
                    format!("`<{open}>` is never closed"),
                ));
            }
            children.push(input.parse()?);
        }
    }
}

impl Parse for Attr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.call(Ident::parse_any)?;
        if input.peek(Token![:]) && !input.peek(Token![::]) {
            input.parse::<Token![:]>()?;
            let inner = input.call(Ident::parse_any)?;
            if name == "let" {
                return Ok(Attr::Let(inner));
            }
            let value = if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
                Some(input.parse()?)
            } else {
                None
            };
            return Ok(Attr::Variant(name, inner, value));
        }
        if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            input.parse::<Token![=]>()?;
            return Ok(Attr::Maybe(name, input.parse()?));
        }
        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            return Ok(Attr::Value(name, input.parse()?));
        }
        Ok(Attr::Flag(name))
    }
}

impl Parse for Value {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(Brace) {
            let content;
            let brace = braced!(content in input);
            return block_expr(brace, &content).map(Value::Expr);
        }
        if input.peek(Paren) {
            let content;
            parenthesized!(content in input);
            let mut args = Punctuated::<Expr, Token![,]>::parse_terminated(&content)?;
            if args.len() == 1 && !args.trailing_punct() {
                let only = args.pop().map(Pair::into_value);
                return Ok(Value::Expr(only.expect("one argument")));
            }
            return Ok(Value::Args(args));
        }
        simple_expr(input).map(Value::Expr)
    }
}

impl Value {
    pub(crate) fn args(&self) -> TokenStream {
        match self {
            Value::Expr(expr) => expr.to_token_stream(),
            Value::Args(args) => args.to_token_stream(),
        }
    }

    pub(crate) fn conditional(&self) -> (TokenStream, TokenStream) {
        match self {
            Value::Expr(expr) => (expr.to_token_stream(), TokenStream::new()),
            Value::Args(args) => {
                let mut args = args.iter();
                let condition = args.next().to_token_stream();
                (condition, quote!(#(#args),*))
            }
        }
    }

    pub(crate) fn single(&self) -> TokenStream {
        match self {
            Value::Expr(expr) => expr.to_token_stream(),
            Value::Args(args) => quote!((#args)),
        }
    }
}

fn block_expr(brace: Brace, content: ParseStream<'_>) -> syn::Result<Expr> {
    let mut stmts = content.call(Block::parse_within)?;
    if stmts.len() == 1
        && let Some(Stmt::Expr(_, None)) = stmts.first()
        && let Some(Stmt::Expr(expr, None)) = stmts.pop()
    {
        return Ok(expr);
    }
    Ok(Expr::Block(ExprBlock {
        attrs: Vec::new(),
        label: None,
        block: Block {
            brace_token: brace,
            stmts,
        },
    }))
}

fn simple_expr(input: ParseStream<'_>) -> syn::Result<Expr> {
    let mut tokens = TokenStream::new();
    if input.peek(Token![-]) {
        input.parse::<Token![-]>()?.to_tokens(&mut tokens);
    }
    if input.peek(Lit) {
        input.parse::<Lit>()?.to_tokens(&mut tokens);
    } else if input.peek(Bracket) {
        tokens.append(input.parse::<TokenTree>()?);
    } else {
        if input.peek(Token![::]) {
            input.parse::<Token![::]>()?.to_tokens(&mut tokens);
        }
        let head = input.fork().call(Ident::parse_any).ok();
        match head {
            Some(head) if !EXPRESSION_KEYWORDS.contains(&head.to_string().as_str()) => {
                input.call(Ident::parse_any)?.to_tokens(&mut tokens);
            }
            _ => {
                return Err(input.error(
                    "only literals, paths and calls can be written bare; wrap this value in `{…}`",
                ));
            }
        }
    }
    loop {
        if input.peek(Token![::]) {
            let ahead = input.fork();
            ahead.parse::<Token![::]>()?;
            if !ahead.peek(Ident::peek_any) {
                return Err(input.error("wrap a value with generics in `{…}`"));
            }
            input.parse::<Token![::]>()?.to_tokens(&mut tokens);
            input.call(Ident::parse_any)?.to_tokens(&mut tokens);
        } else if input.peek(Token![.]) && (input.peek2(Ident::peek_any) || input.peek2(Lit)) {
            input.parse::<Token![.]>()?.to_tokens(&mut tokens);
            tokens.append(input.parse::<TokenTree>()?);
        } else if input.peek(Paren) || input.peek(Bracket) {
            tokens.append(input.parse::<TokenTree>()?);
        } else {
            return syn::parse2(tokens);
        }
    }
}

impl ToTokens for Element {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Element {
            name,
            attrs,
            children,
        } = self;
        tokens.extend(quote!(< #name #(#attrs)* > #(#children)* < / #name >));
    }
}

impl ToTokens for Attr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            Attr::Flag(name) => quote!(#name),
            Attr::Value(name, value) => quote!(#name = #value),
            Attr::Variant(ns, name, None) => quote!(#ns : #name),
            Attr::Variant(ns, name, Some(value)) => quote!(#ns : #name = #value),
            Attr::Maybe(name, value) => quote!(#name ? = #value),
            Attr::Let(name) => quote!(let : #name),
        });
    }
}

impl ToTokens for Value {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            Value::Expr(expr) => quote!({ #expr }),
            Value::Args(args) => quote!((#args)),
        });
    }
}

impl ToTokens for Node {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            Node::Element(element) => element.to_token_stream(),
            Node::Text(text) => text.to_token_stream(),
            Node::Expr(expr) => quote!({ #expr }),
            Node::Spread(expr) => quote!({ .. #expr }),
        });
    }
}

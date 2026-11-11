use crate::node::{Attr, Element, Node, Nodes};
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Block, Expr, ExprIf, Ident, LitInt, LitStr, Stmt, Token, bracketed, parenthesized};

pub(crate) fn view(input: TokenStream) -> syn::Result<TokenStream> {
    let Nodes(nodes) = syn::parse2(input)?;
    if nodes.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "`view!` needs at least one node",
        ));
    }
    Expander { root: root()? }.views(&nodes)
}

pub(crate) fn element(input: TokenStream) -> syn::Result<TokenStream> {
    let call: ElementCall = syn::parse2(input)?;
    Expander { root: call.root }.element(&call.component, &call.element)
}

fn root() -> syn::Result<TokenStream> {
    for (package, path) in [("rouge-core", None), ("rouge", Some(quote!(::core)))] {
        let name = match crate_name(package) {
            Ok(FoundCrate::Itself) => package.replace('-', "_"),
            Ok(FoundCrate::Name(name)) => name,
            Err(_) => continue,
        };
        let name = Ident::new(&name, Span::call_site());
        return Ok(quote!(::#name #path));
    }
    Err(syn::Error::new(
        Span::call_site(),
        "`view!` needs `rouge` or `rouge-core` as a dependency",
    ))
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Children {
    None,
    View,
    Value,
    Lazy(usize),
}

impl ToTokens for Children {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            Children::None => quote!(none),
            Children::View => quote!(view),
            Children::Value => quote!(value),
            Children::Lazy(arity) => {
                let arity = LitInt::new(&arity.to_string(), Span::call_site());
                quote!(lazy #arity)
            }
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Fallback {
    Required,
    Default,
    Flag,
    Optional,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Convert {
    Never,
    Into,
    Reactive,
}

pub(crate) struct Param {
    pub(crate) name: Ident,
    pub(crate) convert: Convert,
    pub(crate) fallback: Fallback,
}

impl Param {
    pub(crate) fn new(name: Ident) -> Self {
        Param {
            name,
            convert: Convert::Never,
            fallback: Fallback::Required,
        }
    }

    fn convert(&self, root: &TokenStream, value: TokenStream) -> TokenStream {
        let value = match self.convert {
            Convert::Into => quote!(::core::convert::Into::into(#value)),
            Convert::Reactive => quote!(#root::IntoProp::into_prop(#value)),
            Convert::Never => value,
        };
        if self.fallback == Fallback::Optional {
            quote!(::core::option::Option::Some(#value))
        } else {
            value
        }
    }

    fn maybe(&self, root: &TokenStream, value: TokenStream) -> syn::Result<TokenStream> {
        if self.fallback != Fallback::Optional {
            return Err(syn::Error::new_spanned(
                &self.name,
                format!(
                    "`{}` is not optional, so it cannot be set with `?=`",
                    self.name
                ),
            ));
        }
        Ok(match self.convert {
            Convert::Into => {
                quote!(::core::option::Option::map(#value, ::core::convert::Into::into))
            }
            Convert::Reactive => {
                quote!(::core::option::Option::map(#value, #root::IntoProp::into_prop))
            }
            Convert::Never => value,
        })
    }

    fn missing(&self) -> Option<TokenStream> {
        match self.fallback {
            Fallback::Optional => Some(quote!(::core::option::Option::None)),
            Fallback::Default | Fallback::Flag => Some(quote!(::core::default::Default::default())),
            Fallback::Required => None,
        }
    }
}

impl ToTokens for Param {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let name = &self.name;
        let fallback = match self.fallback {
            Fallback::Required => None,
            Fallback::Default => Some("default"),
            Fallback::Flag => Some("flag"),
            Fallback::Optional => Some("optional"),
        };
        let convert = match self.convert {
            Convert::Never => None,
            Convert::Into => Some("into"),
            Convert::Reactive => Some("reactive"),
        };
        let flags = convert
            .into_iter()
            .chain(fallback)
            .map(|flag| Ident::new(flag, Span::call_site()));
        tokens.extend(quote!(#name(#(#flags)*)));
    }
}

impl Parse for Param {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut param = Param::new(input.parse()?);
        let flags;
        parenthesized!(flags in input);
        while !flags.is_empty() {
            let flag: Ident = flags.parse()?;
            match flag.to_string().as_str() {
                "into" => param.convert = Convert::Into,
                "reactive" => param.convert = Convert::Reactive,
                "default" => param.fallback = Fallback::Default,
                "flag" => param.fallback = Fallback::Flag,
                "optional" => param.fallback = Fallback::Optional,
                _ => return Err(syn::Error::new_spanned(flag, "unknown parameter flag")),
            }
        }
        Ok(param)
    }
}

struct Component {
    params: Vec<Param>,
    children: Children,
}

struct ElementCall {
    root: TokenStream,
    component: Component,
    element: Element,
}

impl Parse for ElementCall {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let root;
        parenthesized!(root in input);
        let params;
        bracketed!(params in input);
        let params = Punctuated::<Param, Token![,]>::parse_terminated(&params)?;
        let kind: Ident = input.parse()?;
        let children = match kind.to_string().as_str() {
            "none" => Children::None,
            "view" => Children::View,
            "value" => Children::Value,
            "lazy" => Children::Lazy(input.parse::<LitInt>()?.base10_parse()?),
            _ => return Err(syn::Error::new_spanned(kind, "unknown children kind")),
        };
        Ok(ElementCall {
            root: root.parse()?,
            component: Component {
                params: params.into_iter().collect(),
                children,
            },
            element: input.parse()?,
        })
    }
}

struct Expander {
    root: TokenStream,
}

impl Expander {
    fn element(&self, component: &Component, element: &Element) -> syn::Result<TokenStream> {
        let Element {
            name,
            attrs,
            children,
        } = element;
        let mut args: Vec<Option<TokenStream>> = vec![None; component.params.len()];
        let mut methods: Vec<Method<'_>> = Vec::new();
        let mut variants: Vec<(&Ident, TokenStream)> = Vec::new();
        let mut lets = Vec::new();
        let find = |name: &Ident| component.params.iter().position(|p| p.name == *name);
        let mut assign = |index: usize, value: TokenStream| -> syn::Result<()> {
            if args[index].replace(value).is_some() {
                let param = &component.params[index].name;
                return Err(syn::Error::new_spanned(
                    param,
                    format!("`{param}` is set more than once"),
                ));
            }
            Ok(())
        };
        for attr in attrs {
            match attr {
                Attr::Let(binding) => lets.push(binding),
                Attr::Flag(flag) => match find(flag) {
                    Some(index) => {
                        let param = &component.params[index];
                        let value = if param.fallback == Fallback::Flag {
                            quote!(true)
                        } else {
                            param.convert(&self.root, flag.to_token_stream())
                        };
                        assign(index, value)?;
                    }
                    None => methods.push(Method::Always(quote!(.#flag()))),
                },
                Attr::Value(method, value) => {
                    if let Some(index) = find(method) {
                        assign(
                            index,
                            component.params[index].convert(&self.root, value.single()),
                        )?;
                    } else {
                        let value = value.args();
                        methods.push(Method::Always(quote!(.#method(#value))));
                    }
                }
                Attr::Maybe(method, value) => match find(method) {
                    Some(index) => {
                        assign(
                            index,
                            component.params[index].maybe(&self.root, value.single())?,
                        )?;
                    }
                    None => methods.push(Method::Some(method, value.single())),
                },
                Attr::Variant(ns, method, Some(value)) if ns == "when" => {
                    let (condition, args) = value.conditional();
                    methods.push(Method::When(method, condition, args));
                }
                Attr::Variant(ns, method, value) => {
                    let value = value.as_ref().map(crate::node::Value::args);
                    let call = quote!(.#method(#value));
                    match variants.iter_mut().find(|(existing, _)| *existing == ns) {
                        Some((_, calls)) => calls.extend(call),
                        None => variants.push((ns, call)),
                    }
                }
            }
        }
        let slot = find(&Ident::new("children", Span::call_site()));
        let arity = match component.children {
            Children::Lazy(arity) => arity,
            _ => 0,
        };
        if let Some(extra) = lets.get(arity) {
            return Err(syn::Error::new_spanned(
                extra,
                format!(
                    "`<{}>` does not pass a value to bind here",
                    name.to_token_stream()
                ),
            ));
        }
        let mut appended = TokenStream::new();
        match (component.children, slot) {
            (Children::None, _) | (_, None) => {
                for child in children {
                    appended.extend(match child {
                        Node::Spread(iter) => quote!(.children(#iter)),
                        child => {
                            let child = self.view(child)?;
                            quote!(.child(#child))
                        }
                    });
                }
            }
            (kind, Some(index)) if !children.is_empty() => {
                if args[index].is_some() {
                    return Err(syn::Error::new_spanned(
                        name,
                        "`children` is set both as an attribute and as content",
                    ));
                }
                let content = self.slot(kind, &lets, children, name)?;
                args[index] = Some(if kind == Children::Value {
                    component.params[index].convert(&self.root, content)
                } else {
                    content
                });
            }
            _ => {}
        }
        for (param, arg) in component.params.iter().zip(&mut args) {
            if arg.is_none() {
                *arg = param.missing();
            }
        }
        let missing: Vec<String> = component
            .params
            .iter()
            .zip(&args)
            .filter(|(_, arg)| arg.is_none())
            .map(|(param, _)| format!("`{}`", param.name))
            .collect();
        if !missing.is_empty() {
            return Err(syn::Error::new_spanned(
                name,
                format!(
                    "`<{}>` is missing {}",
                    name.to_token_stream(),
                    missing.join(", ")
                ),
            ));
        }
        let style = Ident::new("__rouge_style", Span::mixed_site());
        let variants = variants
            .into_iter()
            .map(|(ns, calls)| quote!(.#ns(|#style| #style #calls)));
        let args = args.into_iter().flatten();
        let target = Ident::new("__rouge_target", Span::mixed_site());
        let found = Ident::new("__rouge_value", Span::mixed_site());
        let mut built = quote!(#name(#(#args),*));
        for method in methods {
            built = match method {
                Method::Always(call) => quote!(#built #call),
                Method::Some(method, value) => quote!({
                    let #target = #built;
                    match #value {
                        ::core::option::Option::Some(#found) => #target.#method(#found),
                        ::core::option::Option::None => #target,
                    }
                }),
                Method::When(method, condition, args) => quote!({
                    let #target = #built;
                    if #condition { #target.#method(#args) } else { #target }
                }),
            };
        }
        Ok(quote!(#built #(#variants)* #appended))
    }

    fn slot(
        &self,
        kind: Children,
        lets: &[&Ident],
        children: &[Node],
        name: &syn::Path,
    ) -> syn::Result<TokenStream> {
        match kind {
            Children::None => unreachable!("handled by the caller"),
            Children::View => self.views(children),
            Children::Value => match children {
                [Node::Text(text)] => Ok(text_content(text)),
                [Node::Spread(iter)] => {
                    Err(syn::Error::new_spanned(iter, "a spread needs a container"))
                }
                [Node::Expr(expr)] => Ok(expr.to_token_stream()),
                [child] => self.view(child),
                _ => Err(syn::Error::new_spanned(
                    name,
                    format!("`<{}>` takes a single child", name.to_token_stream()),
                )),
            },
            Children::Lazy(arity) => {
                let views = self.views(children)?;
                let params = (0..arity).map(|index| {
                    lets.get(index)
                        .map_or_else(|| quote!(_), ToTokens::to_token_stream)
                });
                Ok(quote!(move |#(#params),*| #views))
            }
        }
    }

    fn views(&self, nodes: &[Node]) -> syn::Result<TokenStream> {
        if let [node] = nodes {
            return self.view(node);
        }
        let root = &self.root;
        let views = nodes
            .iter()
            .map(|node| self.view(node))
            .collect::<syn::Result<Vec<_>>>()?;
        Ok(quote!(::std::vec![#(#root::View::into_any(#views)),*]))
    }

    fn view(&self, node: &Node) -> syn::Result<TokenStream> {
        let root = &self.root;
        Ok(match node {
            Node::Element(element) => {
                let name = &element.name;
                quote!(#name! { (#root) #element })
            }
            Node::Text(text) => {
                let content = text_content(text);
                quote!(#root::text(#content))
            }
            Node::Expr(expr) => self.erase(expr),
            Node::Spread(iter) => quote!(::std::iter::Iterator::collect::<::std::vec::Vec<_>>(
                ::std::iter::IntoIterator::into_iter(#iter)
            )),
        })
    }
}

enum Method<'a> {
    Always(TokenStream),
    Some(&'a Ident, TokenStream),
    When(&'a Ident, TokenStream, TokenStream),
}

fn unbraced(block: &Block) -> TokenStream {
    match block.stmts.as_slice() {
        [Stmt::Expr(expr, None)] => expr.to_token_stream(),
        _ => block.to_token_stream(),
    }
}

fn has_final_else(expr: &ExprIf) -> bool {
    match expr.else_branch.as_ref().map(|(_, branch)| &**branch) {
        Some(Expr::If(next)) => has_final_else(next),
        Some(_) => true,
        None => false,
    }
}

impl Expander {
    fn erase(&self, expr: &Expr) -> TokenStream {
        let root = &self.root;
        match expr {
            Expr::If(branch) => self.erase_if(branch, !has_final_else(branch)),
            Expr::Match(matched) => {
                let (subject, arms) = (&matched.expr, &matched.arms);
                let arms = arms.iter().map(|arm| {
                    let (pat, body) = (&arm.pat, &arm.body);
                    let guard = arm.guard.as_ref().map(|(_, guard)| quote!(if #guard));
                    quote!(#pat #guard => #root::View::into_any(#body),)
                });
                quote!(match #subject { #(#arms)* })
            }
            other => other.to_token_stream(),
        }
    }

    fn erase_if(&self, branch: &ExprIf, optional: bool) -> TokenStream {
        let root = &self.root;
        let (condition, then) = (&branch.cond, &branch.then_branch);
        let wrap = |body: TokenStream| {
            let erased = quote!(#root::View::into_any(#body));
            if optional {
                quote!(::core::option::Option::Some(#erased))
            } else {
                erased
            }
        };
        let then = wrap(unbraced(then));
        let otherwise = match branch.else_branch.as_ref().map(|(_, branch)| &**branch) {
            Some(Expr::If(next)) => self.erase_if(next, optional),
            Some(Expr::Block(block)) if block.label.is_none() => {
                let body = wrap(unbraced(&block.block));
                quote!({ #body })
            }
            Some(other) => {
                let body = wrap(other.to_token_stream());
                quote!({ #body })
            }
            None => quote!({ ::core::option::Option::None }),
        };
        quote!(if #condition { #then } else #otherwise)
    }
}

fn text_content(text: &LitStr) -> TokenStream {
    if has_holes(&text.value()) {
        quote!(move || ::std::format!(#text))
    } else {
        text.to_token_stream()
    }
}

fn has_holes(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
            } else {
                return true;
            }
        }
    }
    false
}

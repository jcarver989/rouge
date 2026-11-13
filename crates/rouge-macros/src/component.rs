use crate::view::{Children, Convert, Fallback, Param};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{
    FnArg, GenericParam, Generics, Ident, ItemFn, Pat, PathArguments, Token, Type, TypeParamBound,
    Visibility, WherePredicate,
};

pub(crate) fn component(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    if !args.is_empty() {
        return Err(syn::Error::new_spanned(
            args,
            "`#[component]` takes no arguments",
        ));
    }
    let mut function: ItemFn = syn::parse2(item)?;
    let name = function.sig.ident.clone();
    let mut params = Vec::new();
    let mut children = Children::None;
    let generics = function.sig.generics.clone();
    for input in &mut function.sig.inputs {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "a component is a free function",
            ));
        };
        let Pat::Ident(pat) = &*typed.pat else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "component parameters must be plain identifiers",
            ));
        };
        if pat.ident == "children" {
            children = kind(&typed.ty, &generics);
        }
        let mut param = Param::new(pat.ident.clone());
        let mut kept = Vec::new();
        for attr in typed.attrs.drain(..) {
            if !attr.path().is_ident("prop") {
                kept.push(attr);
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("into") {
                    param.convert = Convert::Into;
                } else if meta.path.is_ident("reactive") {
                    param.convert = Convert::Reactive;
                } else if meta.path.is_ident("default") {
                    param.fallback = Fallback::Default;
                } else if meta.path.is_ident("optional") {
                    param.fallback = Fallback::Optional;
                } else {
                    return Err(meta.error("expected `into`, `reactive`, `default` or `optional`"));
                }
                Ok(())
            })?;
        }
        typed.attrs = kept;
        if is_named(&typed.ty, "Option") {
            param.fallback = Fallback::Optional;
        } else if param.fallback == Fallback::Default && is_named(&typed.ty, "bool") {
            param.fallback = Fallback::Flag;
        }
        params.push(param);
    }
    let hidden = format_ident!("__rouge_component_{}", name);
    let export = matches!(function.vis, Visibility::Public(_)).then(|| quote!(#[macro_export]));
    let vis = &function.vis;
    Ok(quote! {
        #[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
        #function

        #[doc(hidden)]
        #export
        macro_rules! #hidden {
            (($($root:tt)*) $($element:tt)*) => {
                $($root)*::__element! { ($($root)*) [#(#params),*] #children $($element)* }
            };
        }

        #[doc(hidden)]
        #[allow(unused_imports)]
        #vis use #hidden as #name;
    })
}

fn is_named(ty: &Type, name: &str) -> bool {
    matches!(ty, Type::Path(path) if path.qself.is_none()
        && path.path.segments.last().is_some_and(|segment| segment.ident == name))
}

fn kind(ty: &Type, generics: &Generics) -> Children {
    match ty {
        Type::ImplTrait(ty) => from_bounds(&ty.bounds),
        Type::Path(path) if path.qself.is_none() => {
            let Some(ident) = path.path.get_ident() else {
                return Children::Value;
            };
            let mut bounds = Punctuated::<TypeParamBound, Token![+]>::new();
            bounds.extend(bounds_of(ident, generics));
            from_bounds(&bounds)
        }
        _ => Children::Value,
    }
}

fn bounds_of(ident: &Ident, generics: &Generics) -> Vec<TypeParamBound> {
    let declared = generics.params.iter().filter_map(|param| match param {
        GenericParam::Type(param) if param.ident == *ident => Some(param.bounds.iter()),
        _ => None,
    });
    let predicates = generics
        .where_clause
        .iter()
        .flat_map(|clause| &clause.predicates)
        .filter_map(|predicate| match predicate {
            WherePredicate::Type(predicate)
                if matches!(&predicate.bounded_ty, Type::Path(path) if path.path.is_ident(ident)) =>
            {
                Some(predicate.bounds.iter())
            }
            _ => None,
        });
    declared.chain(predicates).flatten().cloned().collect()
}

fn from_bounds(bounds: &Punctuated<TypeParamBound, Token![+]>) -> Children {
    let mut view = false;
    for bound in bounds {
        let TypeParamBound::Trait(bound) = bound else {
            continue;
        };
        let Some(segment) = bound.path.segments.last() else {
            continue;
        };
        if let PathArguments::Parenthesized(args) = &segment.arguments
            && ["Fn", "FnMut", "FnOnce"].iter().any(|f| segment.ident == f)
        {
            return Children::Lazy(args.inputs.len());
        }
        view |= segment.ident == "View";
    }
    if view {
        Children::View
    } else {
        Children::Value
    }
}

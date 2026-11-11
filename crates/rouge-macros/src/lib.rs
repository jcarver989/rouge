mod component;
mod node;
mod view;

use proc_macro::TokenStream;

#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    view::view(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[doc(hidden)]
#[proc_macro]
pub fn __element(input: TokenStream) -> TokenStream {
    view::element(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn component(args: TokenStream, item: TokenStream) -> TokenStream {
    component::component(args.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

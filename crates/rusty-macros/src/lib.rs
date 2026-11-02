//! Procedural macros for the Rusty framework.
//!
//! Provides [`view!`] for JSX-like element construction and [`component`] for
//! transforming a function into a typed component.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod ast;
mod codegen;
mod component;
mod parser;

use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Builds a Rusty `Element` tree from JSX-like Rust syntax.
///
/// # Supported syntax
///
/// - Nested HTML elements: `<main><h1>...</h1></main>`
/// - String-literal text: `<p>"Hello"</p>`
/// - Dynamic expressions: `<p>{message}</p>`
/// - Static and dynamic attributes: `<a href="/about">`, `<div class={name}>`
/// - Boolean attributes: `<input disabled>`
/// - Self-closing and void elements: `<br>`, `<img src="/logo.png" />`
/// - Fragments: multiple top-level nodes
/// - Custom element names: `<my-widget>...</my-widget>`
/// - Components: `<UserCard user={user.clone()} />`
/// - Children: `<Layout><p>"Content"</p></Layout>`
/// - Conditionals: `{#if condition} ... {:else} ... {/if}`
/// - Iteration: `{#each items as item} ... {/each}`
///
/// # Example
///
/// ```rust,ignore
/// use rusty::prelude::*;
///
/// let page = view! {
///     <main>
///         <h1>"Hello from Rusty"</h1>
///     </main>
/// };
/// ```
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    let view = parse_macro_input!(input as ast::View);
    codegen::expand(&view).into()
}

/// Transforms a function into a typed Rusty component.
///
/// Function parameters become public fields on a generated component struct with
/// the same name. The function body becomes the component's render method. Use
/// the component as a tag:
///
/// ```rust,ignore
/// use rusty::prelude::*;
///
/// #[component]
/// pub fn Greeting(name: String) -> Element {
///     view! { <p>{format!("Hello, {name}!")}</p> }
/// }
///
/// let page = view! { <Greeting name="Rusty" /> };
/// ```
///
/// Components take ownership of their props. A component that accepts children
/// should declare a `children: Children` parameter. Use self-closing tags for
/// components without children; paired tags always pass nested content, even
/// when empty. Generic and async components are rejected with a clear diagnostic
/// until the milestones that support them.
#[proc_macro_attribute]
pub fn component(attributes: TokenStream, item: TokenStream) -> TokenStream {
    if !attributes.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "`#[component]` does not accept arguments",
        )
        .to_compile_error()
        .into();
    }

    let function = parse_macro_input!(item as syn::ItemFn);
    component::expand(function)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

//! Procedural macros for the Rusty framework.
//!
//! Milestone 1 provides [`view!`], a JSX-like macro for nested HTML elements.
//! Component, conditional, and iteration syntax is intentionally rejected until
//! the milestone that implements it.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod ast;
mod codegen;
mod parser;

use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Builds a Rusty `Element` tree from JSX-like Rust syntax.
///
/// # Supported syntax
///
/// - Nested HTML elements: `<main><h1>...</h1></main>`
/// - String-literal text: `<p>"Hello"</p>`
/// - Static attributes: `<a href="/about">...</a>`
/// - Boolean attributes: `<input disabled>`
/// - Self-closing and void elements: `<br>`, `<img src="/logo.png" />`
/// - Fragments: multiple top-level nodes
/// - Custom element names: `<my-widget>...</my-widget>`
///
/// # Not yet supported
///
/// Components (tags beginning with an uppercase letter) and dynamic `{ ... }`
/// expressions produce compile-time errors until Milestone 2. This is deliberate:
/// unsupported syntax fails loudly instead of rendering incorrectly.
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

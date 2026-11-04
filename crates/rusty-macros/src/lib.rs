//! Procedural macros for the Rusty framework.
//!
//! Provides [`view!`] for JSX-like element construction, [`component`] for
//! transforming a function into a typed component, and [`server`] for typed
//! server functions.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod app;
mod ast;
mod codegen;
mod component;
mod parser;
mod server;

use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Builds a file-based application module tree.
///
/// Apply this attribute to an empty module declaration. The macro scans
/// `src/app` (or the configured application directory), declares a module for
/// every page, layout, error boundary, not-found page, and API route, and
/// generates `router()` and `serve()` functions:
///
/// ```rust,ignore
/// use rusty::prelude::*;
///
/// #[rusty::app]
/// mod app {}
///
/// fn main() -> Result<(), rusty::app::ServeError> {
///     app::serve()
/// }
/// ```
///
/// Page modules export `pub fn page(context: &mut RequestContext) ->
/// Result<Element, ServerError>` or an `async fn` with the same signature.
/// Layouts export `pub fn layout(children: Children) -> Element`. API routes
/// export lower-case HTTP method handlers such as `pub fn get(context: &mut
/// RequestContext) -> Result<Response, ServerError>`; those handlers may also be
/// declared `async`. Loading boundaries export `pub fn loading() -> Element`.
#[proc_macro_attribute]
pub fn app(attributes: TokenStream, item: TokenStream) -> TokenStream {
    if !attributes.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "`#[rusty::app]` does not accept arguments",
        )
        .to_compile_error()
        .into();
    }

    let module = parse_macro_input!(item as syn::ItemMod);
    app::expand(module)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

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

/// Marks a function as a typed server function.
///
/// The function must be synchronous, non-generic, return
/// `Result<T, RpcError>`, and take serializable arguments. The macro generates
/// a hidden marker type implementing `ServerFunction` (and `ServerInvoker` on
/// native targets) plus a WebAssembly client stub with the same signature.
///
/// The default path is `/_rusty/rpc/<function_name>`. Pass a literal to
/// override it:
///
/// ```rust,ignore
/// use rusty::prelude::*;
///
/// #[server]
/// pub fn add(left: i32, right: i32) -> Result<i32, RpcError> {
///     Ok(left + right)
/// }
///
/// #[server("/api/health")]
/// pub fn health() -> Result<String, RpcError> {
///     Ok("ok".to_owned())
/// }
/// ```
#[proc_macro_attribute]
pub fn server(attributes: TokenStream, item: TokenStream) -> TokenStream {
    server::expand(attributes.into(), item.into()).into()
}

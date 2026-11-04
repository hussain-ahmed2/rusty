//! Expansion for the `#[rusty::app]` module macro.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{ItemMod, LitStr};

use rusty_compiler::AppCompiler;

pub(crate) fn expand(item: ItemMod) -> syn::Result<TokenStream> {
    let project_root = env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            "`#[rusty::app]` requires CARGO_MANIFEST_DIR; run it from a Cargo build",
        )
    })?;
    let compiler = AppCompiler::from_project(&project_root).map_err(app_error)?;
    let manifest = compiler.analyze().map_err(app_error)?;
    let app_root = compiler.app_dir();

    let not_found_layouts = manifest
        .routes()
        .routes()
        .iter()
        .find(|route| route.pattern().is_root())
        .or_else(|| manifest.routes().routes().first())
        .map(|route| {
            route
                .layouts()
                .iter()
                .map(|path| app_root.join(path))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut modules = ModuleMap::default();
    let mut routes = Vec::new();
    for route in manifest.routes().routes() {
        let page = modules.register(&app_root, route.page())?;
        let layouts = route
            .layouts()
            .iter()
            .map(|path| modules.register(&app_root, path))
            .collect::<syn::Result<Vec<_>>>()?;
        let loading = route
            .loading()
            .map(|path| modules.register(&app_root, path))
            .transpose()?;
        let error = route
            .error()
            .map(|path| modules.register(&app_root, path))
            .transpose()?;
        routes.push((route, page, layouts, loading, error));
    }

    let mut apis = Vec::new();
    for route in manifest.routes().api_routes() {
        let module = modules.register(&app_root, route.route())?;
        apis.push((route, module));
    }

    let not_found = manifest
        .routes()
        .not_found()
        .map(|path| modules.register(&app_root, path))
        .transpose()?;

    let mut rpcs = Vec::new();
    for function in manifest.server_functions() {
        let source = compiler.project_root().join(function.source());
        let marker = format_ident!("{}ServerFn", pascal_case(function.name()));
        let marker_path = if let Ok(relative) = source.strip_prefix(&app_root) {
            let module = modules.register(&app_root, relative)?;
            let module = modules.ident(&module);
            quote! { #module::#marker }
        } else {
            let module: syn::Path = syn::parse_str(function.module()).map_err(|_| {
                syn::Error::new(
                    Span::call_site(),
                    format!(
                        "invalid module path `{}` for server function `{}`",
                        function.module(),
                        function.name()
                    ),
                )
            })?;
            quote! { #module::#marker }
        };
        rpcs.push(quote! { router = router.rpc::<#marker_path>(); });
    }

    let mut registrations = Vec::new();
    for (route, page, layouts, loading, error) in routes {
        let pattern = route.pattern().to_string();
        let page_async = modules.is_async(&page, "page");
        let page = modules.ident(&page);
        let layouts = layouts
            .iter()
            .map(|module| modules.ident(module))
            .collect::<Vec<_>>();
        let loading = match loading {
            Some(loading) => {
                let loading = modules.ident(&loading);
                quote! { .loading(#loading::loading) }
            }
            None => TokenStream::new(),
        };
        let error = match error {
            Some(error) => {
                let error = modules.ident(&error);
                quote! { .error(#error::error) }
            }
            None => TokenStream::new(),
        };
        let page_route = if page_async {
            quote! {
                ::rusty::app::PageRoute::async_new(
                    #pattern,
                    ::std::sync::Arc::new(
                        |context| ::std::boxed::Box::pin(#page::page(context)),
                    ),
                )
            }
        } else {
            quote! { ::rusty::app::PageRoute::new(#pattern, #page::page) }
        };
        registrations.push(quote! {
            router = router.page(
                #page_route
                    .layouts(&[#(#layouts::layout),*])
                    #loading
                    #error
            );
        });
    }

    for (route, module) in apis {
        let pattern = route.pattern().to_string();
        let names = modules.functions(&module);
        let get = api_method(names, "get", "GET");
        let post = api_method(names, "post", "POST");
        let put = api_method(names, "put", "PUT");
        let patch = api_method(names, "patch", "PATCH");
        let delete = api_method(names, "delete", "DELETE");
        let options = api_method(names, "options", "OPTIONS");
        if [get, post, put, patch, delete, options]
            .iter()
            .all(Option::is_none)
        {
            return Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "API route `{pattern}` must export at least one of `get`, `post`, `put`, `patch`, `delete`, or `options`"
                ),
            ));
        }
        let module = modules.ident(&module);
        let get = api_handler(&module, get);
        let post = api_handler(&module, post);
        let put = api_handler(&module, put);
        let patch = api_handler(&module, patch);
        let delete = api_handler(&module, delete);
        let options = api_handler(&module, options);
        registrations.push(quote! {
            router = router.api(
                #pattern,
                ::rusty::app::ApiMethods {
                    get: #get,
                    post: #post,
                    put: #put,
                    patch: #patch,
                    delete: #delete,
                    options: #options,
                },
            );
        });
    }

    registrations.extend(rpcs);

    if let Some(not_found) = not_found {
        let not_found = modules.ident(&not_found);
        let layouts = not_found_layouts
            .iter()
            .map(|path| modules.ident(path))
            .collect::<Vec<_>>();
        if layouts.is_empty() {
            registrations.push(quote! {
                router = router.not_found(#not_found::not_found);
            });
        } else {
            registrations.push(quote! {
                router = router.not_found_with_layouts(
                    #not_found::not_found,
                    &[#(#layouts::layout),*],
                );
            });
        }
    }

    let generated_modules = modules.entries();
    let attrs = item.attrs;
    let vis = item.vis;
    let ident = item.ident;
    Ok(quote! {
        #(#attrs)*
        #vis mod #ident {
            #(#generated_modules)*

            /// Builds the application route table.
            #[must_use]
            pub fn router() -> ::rusty::app::Router {
                let mut router = ::rusty::app::Router::new();
                #(#registrations)*
                router
            }

            /// Serves the application with `rusty.toml`.
            ///
            /// # Errors
            ///
            /// Returns an error when the application cannot bind or serve.
            pub fn serve() -> Result<(), ::rusty::app::ServeError> {
                ::rusty::app::serve(router())
            }
        }
    })
}

fn api_method(
    names: &BTreeMap<String, FunctionInfo>,
    lower: &'static str,
    upper: &'static str,
) -> Option<(&'static str, bool)> {
    if let Some(function) = names.get(lower) {
        Some((lower, function.is_async))
    } else {
        names.get(upper).map(|function| (upper, function.is_async))
    }
}

fn api_handler(module: &proc_macro2::Ident, method: Option<(&'static str, bool)>) -> TokenStream {
    match method {
        Some((method, true)) => {
            let method = format_ident!("{method}");
            quote! {
                ::core::option::Option::Some(::rusty::app::ApiHandler::Async(
                    ::std::sync::Arc::new(
                        |context| ::std::boxed::Box::pin(#module::#method(context)),
                    ),
                ))
            }
        }
        Some((method, false)) => {
            let method = format_ident!("{method}");
            quote! {
                ::core::option::Option::Some(::rusty::app::ApiHandler::Sync(#module::#method))
            }
        }
        None => quote! { ::core::option::Option::None },
    }
}

fn app_error(error: impl std::fmt::Display) -> syn::Error {
    syn::Error::new(
        Span::call_site(),
        format!("`#[rusty::app]` could not analyze the application: {error}"),
    )
}

#[derive(Default)]
struct ModuleMap {
    modules: BTreeMap<PathBuf, proc_macro2::Ident>,
    functions: BTreeMap<PathBuf, BTreeMap<String, FunctionInfo>>,
}

struct FunctionInfo {
    is_async: bool,
}

impl ModuleMap {
    fn register(&mut self, app_root: &Path, relative: &Path) -> syn::Result<PathBuf> {
        let path = app_root.join(relative);
        if !self.modules.contains_key(&path) {
            let index = self.modules.len();
            let ident = format_ident!("__rusty_route_{index}");
            self.modules.insert(path.clone(), ident);
        }
        if !self.functions.contains_key(&path) {
            let functions = exported_functions(&path)?;
            self.functions.insert(path.clone(), functions);
        }
        Ok(path)
    }

    fn ident(&self, path: &Path) -> proc_macro2::Ident {
        self.modules.get(path).expect("registered module").clone()
    }

    fn functions(&self, path: &Path) -> &BTreeMap<String, FunctionInfo> {
        self.functions.get(path).expect("registered functions")
    }

    fn is_async(&self, path: &Path, name: &str) -> bool {
        self.functions
            .get(path)
            .and_then(|functions| functions.get(name))
            .is_some_and(|function| function.is_async)
    }

    fn entries(&self) -> Vec<TokenStream> {
        self.modules
            .iter()
            .map(|(path, ident)| {
                let path = LitStr::new(&path.to_string_lossy(), Span::call_site());
                quote! {
                    #[path = #path]
                    pub mod #ident;
                }
            })
            .collect()
    }
}

fn exported_functions(path: &Path) -> syn::Result<BTreeMap<String, FunctionInfo>> {
    let source = fs::read_to_string(path).map_err(|error| {
        syn::Error::new(
            Span::call_site(),
            format!("cannot read route module {}: {error}", path.display()),
        )
    })?;
    let file = syn::parse_file(&source).map_err(|error| {
        syn::Error::new(
            Span::call_site(),
            format!("cannot parse route module {}: {error}", path.display()),
        )
    })?;
    let mut functions = BTreeMap::new();
    for item in file.items {
        if let syn::Item::Fn(function) = item {
            if matches!(function.vis, syn::Visibility::Public(_)) {
                functions.insert(
                    function.sig.ident.to_string(),
                    FunctionInfo {
                        is_async: function.sig.asyncness.is_some(),
                    },
                );
            }
        }
    }
    Ok(functions)
}

fn pascal_case(name: &str) -> String {
    let mut output = String::new();
    for segment in name.trim_start_matches("r#").split('_') {
        let mut characters = segment.chars();
        if let Some(first) = characters.next() {
            output.extend(first.to_uppercase());
            output.extend(characters);
        }
    }
    if output.is_empty() {
        output.push_str("Server");
    }
    output
}

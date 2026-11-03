//! Discovery of `#[server]` function declarations.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use syn::{Attribute, Expr, ExprLit, Item, ItemFn, Lit, LitStr, Meta, MetaNameValue};

use crate::error::io;
use crate::manifest::ServerFunction;
use crate::CompileError;

pub(crate) fn discover(
    project_root: &Path,
    source_root: &Path,
) -> Result<Vec<ServerFunction>, CompileError> {
    let mut functions = Vec::new();
    for file in rust_files(source_root)? {
        let source = fs::read_to_string(&file).map_err(|error| io(&file, &error))?;
        let parsed = syn::parse_file(&source).map_err(|error| CompileError::InvalidSource {
            path: file.clone(),
            message: error.to_string(),
        })?;
        let base = module_base(source_root, &file);
        collect_items(&parsed.items, &base, &file, project_root, &mut functions)?;
    }

    functions.sort_by(|left, right| {
        left.path()
            .cmp(right.path())
            .then_with(|| left.name().cmp(right.name()))
            .then_with(|| left.source().cmp(right.source()))
    });

    let mut paths = BTreeMap::<&str, &Path>::new();
    for function in &functions {
        if let Some(first) = paths.insert(function.path(), function.source()) {
            return Err(CompileError::DuplicateServerPath {
                path: function.path().to_owned(),
                first: first.to_path_buf(),
                second: function.source().to_path_buf(),
            });
        }
    }

    Ok(functions)
}

fn rust_files(source_root: &Path) -> Result<Vec<PathBuf>, CompileError> {
    let mut files = Vec::new();
    visit(source_root, true, &mut files)?;
    files.sort();
    Ok(files)
}

fn visit(directory: &Path, is_root: bool, files: &mut Vec<PathBuf>) -> Result<(), CompileError> {
    let entries = fs::read_dir(directory).map_err(|error| io(directory, &error))?;
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io(directory, &error))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| io(&path, &error))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if file_type.is_dir() {
            if is_root && name == "bin" {
                continue;
            }
            if name.starts_with('.') {
                continue;
            }
            visit(&path, false, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            files.push(path);
        }
    }

    Ok(())
}

fn module_base(source_root: &Path, file: &Path) -> String {
    let relative = file.strip_prefix(source_root).unwrap_or(file);
    let file_name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");

    if relative == Path::new("lib.rs") || relative == Path::new("main.rs") {
        return "crate".to_owned();
    }

    let mut segments = Vec::new();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            if let Component::Normal(segment) = component {
                segments.push(segment.to_string_lossy().into_owned());
            }
        }
    }

    if file_name != "mod.rs" && file_name != "lib.rs" && file_name != "main.rs" {
        if let Some(stem) = relative.file_stem().and_then(|stem| stem.to_str()) {
            segments.push(stem.to_owned());
        }
    }

    let mut module = String::from("crate");
    for segment in segments {
        module.push_str("::");
        module.push_str(segment.trim_start_matches("r#"));
    }
    module
}

fn collect_items(
    items: &[Item],
    module: &str,
    file: &Path,
    project_root: &Path,
    functions: &mut Vec<ServerFunction>,
) -> Result<(), CompileError> {
    for item in items {
        match item {
            Item::Fn(function) => {
                collect_function(function, module, file, project_root, functions)?
            }
            Item::Mod(item_mod) => {
                if let Some((_, items)) = &item_mod.content {
                    let name = item_mod.ident.to_string();
                    let module = format!("{module}::{}", name.trim_start_matches("r#"));
                    collect_items(items, &module, file, project_root, functions)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn collect_function(
    function: &ItemFn,
    module: &str,
    file: &Path,
    project_root: &Path,
    functions: &mut Vec<ServerFunction>,
) -> Result<(), CompileError> {
    let mut declarations = Vec::new();
    for attribute in &function.attrs {
        if let Some(path) = server_path(attribute, file)? {
            declarations.push(path);
        }
    }

    let path = match declarations.as_slice() {
        [] => return Ok(()),
        [path] => path.clone(),
        _ => {
            return Err(CompileError::InvalidServerFunction {
                path: file.to_path_buf(),
                message: format!(
                    "function `{}` has multiple `#[server]` attributes",
                    function.sig.ident
                ),
            })
        }
    };

    let name = function
        .sig
        .ident
        .to_string()
        .trim_start_matches("r#")
        .to_owned();
    let path = if path.is_empty() {
        format!("/_rusty/rpc/{name}")
    } else {
        path
    };
    if !path.starts_with('/') {
        return Err(CompileError::InvalidServerFunction {
            path: file.to_path_buf(),
            message: format!("server function `{name}` path must start with `/`"),
        });
    }
    if path.contains(['?', '#']) {
        return Err(CompileError::InvalidServerFunction {
            path: file.to_path_buf(),
            message: format!(
                "server function `{name}` path cannot contain a query string or fragment"
            ),
        });
    }

    functions.push(ServerFunction::new(
        name,
        path,
        module.to_owned(),
        file.strip_prefix(project_root)
            .unwrap_or(file)
            .to_path_buf(),
    ));
    Ok(())
}

fn server_path(attribute: &Attribute, file: &Path) -> Result<Option<String>, CompileError> {
    let Some(segment) = attribute.path().segments.last() else {
        return Ok(None);
    };
    if segment.ident != "server" {
        return Ok(None);
    }

    match &attribute.meta {
        Meta::Path(_) => Ok(Some(String::new())),
        Meta::List(list) => {
            if let Ok(literal) = syn::parse2::<LitStr>(list.tokens.clone()) {
                return Ok(Some(literal.value()));
            }

            let meta: MetaNameValue = syn::parse2(list.tokens.clone()).map_err(|_| {
                invalid(
                    file,
                    "expected `#[server]`, `#[server(\"/path\")]`, or `#[server(path = \"/path\")]`",
                )
            })?;
            if !meta.path.is_ident("path") {
                return Err(invalid(
                    file,
                    "the only supported `#[server]` option is `path`",
                ));
            }
            let Expr::Lit(ExprLit {
                lit: Lit::Str(literal),
                ..
            }) = meta.value
            else {
                return Err(invalid(file, "the `path` option must be a string literal"));
            };
            Ok(Some(literal.value()))
        }
        Meta::NameValue(_) => Err(invalid(
            file,
            "expected `#[server]`, `#[server(\"/path\")]`, or `#[server(path = \"/path\")]`",
        )),
    }
}

fn invalid(file: &Path, message: &str) -> CompileError {
    CompileError::InvalidServerFunction {
        path: file.to_path_buf(),
        message: message.to_owned(),
    }
}

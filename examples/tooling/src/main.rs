//! Analyzes this example application and prints its manifest summary.

#![forbid(unsafe_code)]

mod rpc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = rusty_compiler::AppCompiler::from_project(env!("CARGO_MANIFEST_DIR"))?;
    let manifest = compiler.analyze()?;

    println!(
        "pages {} api {} server {}",
        manifest.routes().len(),
        manifest.routes().api_len(),
        manifest.server_functions().len()
    );
    for route in manifest.routes().routes() {
        println!("{} -> {}", route.pattern(), route.page().display());
    }
    for route in manifest.routes().api_routes() {
        println!("{} -> {} (API)", route.pattern(), route.route().display());
    }
    for function in manifest.server_functions() {
        println!(
            "{} -> {}::{} ({})",
            function.path(),
            function.module(),
            function.name(),
            function.source().display()
        );
    }
    Ok(())
}

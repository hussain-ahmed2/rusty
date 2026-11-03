//! A file-based Rusty application.

#![forbid(unsafe_code)]

mod rpc;

#[rusty::app]
mod app {}

fn main() -> Result<(), rusty::app::ServeError> {
    app::serve()
}

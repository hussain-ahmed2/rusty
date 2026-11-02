# Rusty

> The Next.js experience. The Rust language.

Rusty is a Rust-native full-stack web framework inspired by Next.js. It keeps
the file-system conventions and component mental model familiar while using Rust
for components, server code, and the browser runtime.

Rusty is being built incrementally. The current milestone provides the element
model, the `view!` macro, and safe server-side HTML rendering.

## Status

**Milestone 1 — Foundation (complete)**

- [x] Cargo workspace
- [x] Element model with validated tag and attribute names
- [x] `view!` macro for nested HTML
- [x] Safe HTML serialization
- [x] Hello-world example and tests

Next up: Milestone 2 (component system). Not yet implemented: components,
reactivity, browser runtime, hydration, routing, server functions, compiler, and
CLI. See [docs/PLAN.md](docs/PLAN.md) for the full roadmap and engineering rules.

## Quick start

```bash
cargo test --workspace
cargo run -p hello-world
```

Expected output:

```html
<main><h1>Hello from Rusty</h1><p>A Next.js-inspired framework built with Rust.</p></main>
```

## Example

```rust
use rusty::prelude::*;

fn main() {
    let app = view! {
        <main>
            <h1>"Hello from Rusty"</h1>
            <p>"A Next.js-inspired framework built with Rust."</p>
        </main>
    };

    println!("{}", render_to_string(&app).unwrap());
}
```

## Workspace

| Crate | Purpose |
| --- | --- |
| `rusty` | Public facade and prelude |
| `rusty-core` | Element model and validated names |
| `rusty-macros` | Procedural macros, starting with `view!` |
| `rusty-ssr` | Server-side HTML rendering |

## Project conventions

Rusty applications will use `src/app` for file-system routing:

```text
src/app/
├── layout.rs
├── page.rs
├── loading.rs
├── error.rs
├── not_found.rs
├── about/
│   └── page.rs
└── api/
    └── users/
        └── route.rs
```

This layout is documented now but will only be wired up in Milestone 6.

## License

MIT OR Apache-2.0

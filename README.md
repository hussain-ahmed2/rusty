# Rusty

> The Next.js experience. The Rust language.

Rusty is a Rust-native full-stack web framework inspired by Next.js. It keeps
the file-system conventions and component mental model familiar while using Rust
for components, server code, and the browser runtime.

Rusty is built incrementally. The current milestone provides typed components,
props, children, dynamic expressions, conditionals, iteration, and safe
server-side HTML rendering.

## Status

**Milestone 2 — Component system (complete)**

- [x] Component types generated from ordinary Rust functions
- [x] Typed props with compile-time checking
- [x] Children passed through `Children`
- [x] Dynamic text and attribute expressions
- [x] `{#if}` / `{:else if}` / `{:else}` conditionals
- [x] `{#each}` iteration with destructuring
- [x] Component composition
- [x] Safe escaping and validated HTML names

Next up: Milestone 3 (reactivity). Not yet implemented: signals, browser runtime,
hydration, routing, server functions, compiler, and CLI. See
[docs/PLAN.md](docs/PLAN.md) for the full roadmap and engineering rules.

## Quick start

```bash
cargo test --workspace
cargo run -p hello-world
```

Expected output:

```html
<main class="app"><h1>Hello from Rusty</h1><p>A Next.js-inspired framework built with Rust.</p><ul><li>Components</li><li>Typed props</li><li>Children</li><li>Conditionals</li><li>Iteration</li></ul></main>
```

## Example

```rust
use rusty::prelude::*;

#[component]
fn Layout(children: Children) -> Element {
    view! { <main class="app">{children}</main> }
}

#[component]
fn Greeting(name: String) -> Element {
    view! { <h1>{format!("Hello from {name}")}</h1> }
}

#[component]
fn Home() -> Element {
    view! {
        <Layout>
            <Greeting name="Rusty" />
            <p>"A Next.js-inspired framework built with Rust."</p>
        </Layout>
    }
}

fn main() {
    let app = view! { <Home /> };
    println!("{}", render_to_string(&app).unwrap());
}
```

Components are braced structs at the type level: function parameters become
typed fields (public when the component is public). `<Greeting name="Rusty" />`
is checked like a normal Rust
struct literal, so missing props, unknown props, and wrong prop types are compile
errors. Use self-closing tags for components without children; paired tags always
pass nested content as `children`, even when the body is empty.

## Workspace

| Crate | Purpose |
| --- | --- |
| `rusty` | Public facade and prelude |
| `rusty-core` | Element model, components, children, conversions |
| `rusty-macros` | `view!` and `#[component]` procedural macros |
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

This layout is documented now but will only be wired up in Milestone 6. A
documentation site deployed through GitHub Pages is planned once static
generation exists.

## License

MIT OR Apache-2.0

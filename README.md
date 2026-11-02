# Rusty

> The Next.js experience. The Rust language.

Rusty is a Rust-native full-stack web framework inspired by Next.js. It keeps
the file-system conventions and component mental model familiar while using Rust
for components, server code, and the browser runtime.

Rusty is built incrementally. The current milestone provides typed components,
props, children, dynamic expressions, conditionals, iteration, safe server-side
HTML rendering, and fine-grained reactive state.

## Status

**Milestone 3 — Reactivity (complete)**

- [x] Signals, memos, and effects
- [x] Automatic dependency tracking
- [x] Batched updates and `untrack`
- [x] Cycle-safe ownership with deterministic cleanup
- [x] Context propagation through owner scopes
- [x] Reactive values render their current value in views

Milestone 2 also provides generated component types, typed props, children,
dynamic expressions, conditionals, iteration, and safe escaping. Next up:
Milestone 4 (browser runtime). Not yet implemented: browser DOM updates,
hydration, routing, server functions, compiler, and CLI. See
[docs/PLAN.md](docs/PLAN.md) for the full roadmap and engineering rules.

## Quick start

```bash
cargo test --workspace
cargo run -p hello-world
cargo run -p reactivity
```

Expected `hello-world` output:

```html
<main class="app"><h1>Hello from Rusty</h1><p>A Next.js-inspired framework built with Rust.</p><ul><li>Components</li><li>Typed props</li><li>Children</li><li>Conditionals</li><li>Iteration</li></ul></main>
```

Expected `reactivity` output:

```text
doubled: 2
doubled: 6
<main><h1>Rusty reactivity</h1><p>count = 3</p><p>doubled = 6</p></main>
doubled: 8
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
| `rusty-reactivity` | Signals, memos, effects, ownership, and scheduling |

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

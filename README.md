# Rusty

> The Next.js experience. The Rust language.

Rusty is a Rust-native full-stack web framework inspired by Next.js. It keeps
the file-system conventions and component mental model familiar while using Rust
for components, server code, and the browser runtime.

Rusty is built incrementally. The current milestone provides typed components,
props, children, dynamic expressions, conditionals, iteration, safe server-side
HTML rendering, fine-grained reactive state, a browser runtime that patches DOM
nodes in place, SSR hydration with state restoration, file-based routing with
typed route manifests, and a transport-neutral full-stack runtime with typed
server functions.

## Status

**Milestone 7 — Full stack (complete)**

- [x] Transport-neutral HTTP methods, status codes, headers, requests, and responses
- [x] Method-aware routing with typed parameters and middleware chains
- [x] Bearer authentication and typed request extensions
- [x] Double-submit-cookie CSRF protection
- [x] `#[server]` functions, typed RPC clients, and native registries
- [x] `route.rs` API endpoint discovery alongside page routes
- [x] Native full-stack example

Milestones 1–6 also provide the `view!` macro, typed components, safe escaping,
signals, memos, effects, ownership, context, the DOM abstraction, the client
renderer, reactive roots, hydration, state restoration, and file-based routing.
Next up: Milestone 8 (compiler and tooling). Not yet implemented: keyed list
diffing, the compiler, and CLI. See [docs/PLAN.md](docs/PLAN.md) for the full
roadmap and engineering rules.

## Quick start

```bash
cargo test --workspace
cargo run -p hello-world
cargo run -p reactivity
cargo run -p routing
cargo run -p full-stack
cargo run -q -p rusty-cli -- --help
cargo run -q -p rusty-cli -- new /tmp/rusty-demo
cargo run -q -p rusty-cli -- check --project /tmp/rusty-demo
cargo run -q -p rusty-cli -- generate --project /tmp/rusty-demo
cargo run -q -p rusty-cli -- dev --project /tmp/rusty-demo
cargo run -q -p tooling
rustup target add wasm32-unknown-unknown
cargo build -p browser --target wasm32-unknown-unknown
cargo build -p hydration --target wasm32-unknown-unknown
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

Expected `routing` output:

```text
/ -> page.rs
/blog -> blog/page.rs
/blog/[slug] -> blog/[slug]/page.rs
/docs/[...path] -> docs/[...path]/page.rs
/pricing -> (marketing)/pricing/page.rs
/api/health -> api/health/route.rs (API)
/blog/[slug] captured slug=Some("hello")
```

Expected `full-stack` output:

```text
page / -> page.rs
api  /api/health -> api/health/route.rs
rpc  200 "Hello, Rusty!"
me   200 anonymous
csrf 403 CSRF token missing or invalid
```

Expected `tooling` output:

```text
pages 2 api 1 server 1
/ -> page.rs
/about -> about/page.rs
/api/health -> api/health/route.rs (API)
/_rusty/rpc/ping -> crate::rpc::ping (src/rpc.rs)
```

The browser example is a counter with an `on:click` handler. From
`examples/browser`, run `wasm-pack build --target web` and serve the directory.
Opening `index.html` mounts the counter into the document body. Keep the
`ReactiveRoot` alive for the lifetime of the page; dropping it disposes the
render effect and removes the mounted nodes.

The hydration example embeds server-rendered markers and a state script in
`index.html`. From `examples/hydration`, run `wasm-pack build --target web` and
serve the directory. The client restores `count` from the state script, adopts
the existing nodes, and patches them after a click. Hydration uses the same
`ReactiveRoot` lifetime rules as mounting.

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
| `rusty-ssr` | Server-side HTML and hydration rendering |
| `rusty-reactivity` | Signals, memos, effects, ownership, and scheduling |
| `rusty-vdom` | Virtual node snapshots and hydration markers |
| `rusty-dom` | DOM abstraction with mock and browser backends |
| `rusty-renderer` | Client mounting, patching, hydration, and reactive roots |
| `rusty-hydrate` | Hydration state serialization and restoration |
| `rusty-router` | File-based route discovery, matching, and manifests |
| `rusty-server` | HTTP runtime, HTTP/1.1 transport, middleware, auth, and CSRF |
| `rusty-rpc` | Typed server functions and RPC clients |
| `rusty-config` | `rusty.toml` parsing and validation |
| `rusty-compiler` | Route, server-function, and manifest analysis |
| `rusty-cli` | `rusty new`, `check`, and `generate` |

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

Route discovery and manifests are implemented in Milestone 6. API handlers,
server functions, and application analysis are implemented in Milestones 7 and
8. The development server, file watcher, hot reload, and production build
pipeline remain future work, as do compiler-generated entry points and client
navigation. A documentation site deployed through GitHub Pages is planned once
static generation exists.

## Configuration

`rusty.toml` is optional. Missing files and fields use these defaults:

```toml
[app]
source_root = "src"
app_dir = "src/app"

[build]
out_dir = "target/rusty"

[dev]
host = "127.0.0.1"
port = 3000
```

Paths must be relative to the project root and cannot contain `..`. Unknown
fields are rejected. A development port of `0` asks the operating system for an
ephemeral port. `rusty check` analyzes the application without writing, while
`rusty generate` writes `target/rusty/app-manifest.json`.

`rusty dev` serves `/` as an HTML diagnostics dashboard, `/__rusty/status` and
`/__rusty/manifest` as JSON, and `/__rusty/health` as a health check. It polls
source files and regenerates the manifest as they change. Application route
dispatch and hot reload are still pending compiler-generated runtime
registration; the dev server currently serves diagnostics only.

## License

MIT OR Apache-2.0

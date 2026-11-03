# Rusty

> The Next.js experience. The Rust language.

Rusty is a Rust-native full-stack web framework inspired by Next.js. It keeps
the file-system conventions and component mental model familiar while using Rust
for components, server code, and the browser runtime.

Rusty is built incrementally. The current milestone provides typed components,
props, children, dynamic expressions, conditionals, iteration, safe server-side
HTML rendering, fine-grained reactive state, a browser runtime that patches DOM
nodes in place, SSR hydration with state restoration, file-based routing with
typed route manifests, a transport-neutral full-stack runtime with typed server
functions, static asset serving from a `public` directory, production
WebAssembly client builds through `wasm-bindgen`, and standalone deployment
bundles.

## Status

**Milestone 8 — Compiler and tooling (in progress)**

- [x] Optional `rusty.toml` with typed application, build, and dev settings
- [x] `AppCompiler` route/server-function analysis and validated manifests
- [x] `rusty new`, `check`, `generate`, `build`, `start`, and `dev`
- [x] `#[rusty::app]` compile-time module tree and generated route registry
- [x] Page, layout, error-boundary, not-found, API, and RPC handler contracts
- [x] Application route dispatch, static document wrapping, and API method routing
- [x] Development rebuild-and-restart loop with file watching
- [x] Blocking HTTP/1.1 transport with bounded headers and bodies
- [x] Public static asset serving and validated build-time asset manifests
- [x] WebAssembly client builds and automatic client asset serving
- [x] Standalone deployment bundles with app, public, and client artifacts
- [ ] Loading boundaries, async handlers, and streaming SSR
- [ ] Platform-specific deployment adapters

Milestones 1–7 provide the `view!` macro, typed components, safe escaping,
signals, memos, effects, ownership, context, the DOM abstraction, the client
renderer, reactive roots, hydration, state restoration, file-based routing,
HTTP handling, middleware, auth, CSRF, and server functions. See
[docs/PLAN.md](docs/PLAN.md) for the full roadmap and engineering rules.

## Quick start

```bash
cargo test --workspace
cargo run -p hello-world
cargo run -p reactivity
cargo run -p routing
cargo run -p full-stack
cargo run -q -p rusty-cli -- --help
cargo run -q -p rusty-cli -- new /tmp/rusty-demo
cargo run -q -p rusty-cli -- dev --project /tmp/rusty-demo
cargo run -q -p rusty-cli -- build --project /tmp/rusty-demo
cargo run -q -p rusty-cli -- start --project /tmp/rusty-demo
cargo run -q -p rusty-cli -- check --project /tmp/rusty-demo
cargo run -q -p rusty-cli -- generate --project /tmp/rusty-demo
cargo run -p app
cargo run -p tooling
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
| `rusty-macros` | `view!`, `#[component]`, `#[server]`, and `#[rusty::app]` macros |
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
| `rusty-cli` | `rusty new`, `check`, `generate`, `build`, `start`, and `dev` |

## Project conventions

Rusty applications use `src/app` for file-system routing:

```text
src/app/
├── layout.rs
├── page.rs
├── error.rs
├── not_found.rs
├── about/
│   └── page.rs
└── api/
    └── users/
        └── route.rs
public/
├── robots.txt
└── assets/
    └── site.css
```

The `#[rusty::app]` attribute scans that tree at compile time and generates the
module declarations and route registry:

```rust
#[rusty::app]
mod app {}

fn main() -> Result<(), rusty::app::ServeError> {
    app::serve()
}
```

Files under `public` are served at the site root: `public/robots.txt` is available
at `/robots.txt`, and `public/assets/site.css` at `/assets/site.css`. Static files
are checked after application routes and before the not-found page, so explicit
routes always win. A missing `public` directory simply disables static serving.
`serve()` applies the configured public directory automatically; custom servers
built from `router()` should call `.public_dir(...)`.

The generated module exports `router()` for tests or custom servers and `serve()`
for the standard HTTP runtime. Page modules export:

```rust
pub fn page(context: &mut RequestContext) -> Result<Element, ServerError>
```

Layouts export `pub fn layout(children: Children) -> Element`, error boundaries
export `pub fn error(error: &ServerError) -> Element`, and the root not-found
page exports `pub fn not_found() -> Element`. API route modules export lower-case
HTTP methods such as `pub fn get(context: &mut RequestContext) ->
Result<Response, ServerError>`. `#[server]` markers discovered outside `src/app`
are registered automatically from their declared module paths.

Every application must include a build script that emits
`cargo:rerun-if-changed` for `src` so Cargo re-expands the macro when routes
are added, removed, or changed. `rusty new` generates one; `rusty new --client`
also scaffolds a WebAssembly client package and its layout bootstrap.

The development supervisor runs the application with `cargo run`, sets
`RUSTY_DEV=1`, and restarts it when `.rs`, `Cargo.toml`, or `rusty.toml` files
change. `rusty build` writes the application manifest and delegates native
compilation to Cargo; `rusty start` runs it. Static responses use MIME types
derived from file extensions, `Cache-Control: public, max-age=3600` in production,
and `no-cache` in development. Loading boundaries, async handlers, client
navigation, and deployment adapters remain future milestones.

## Configuration

`rusty.toml` is optional. Missing files and fields use these defaults:

```toml
[app]
source_root = "src"
app_dir = "src/app"
public_dir = "public"

[build]
out_dir = "target/rusty"
# client_dir = "client"

[dev]
host = "127.0.0.1"
port = 3000
```

Paths must be relative to the project root and cannot contain `..`. Unknown
fields are rejected. A development port of `0` asks the operating system for an
ephemeral port. `rusty check` analyzes the application without writing, while
`rusty generate` writes `target/rusty/app-manifest.json`. The optional
`build.client_dir` points at a Cargo package that produces a `cdylib` target.

`rusty dev` starts the application with `RUSTY_DEV=1` and restarts it when
watched files change. In development the runtime serves `/__rusty/health` and
`/__rusty/status`; status reports the generated page, API, and server-function
counts. `rusty build` writes the asset manifest before compiling.

### WebAssembly client builds

`rusty new <path> --client` creates a client package, adds `client_dir = "client"`
to `rusty.toml`, injects the module bootstrap into the root layout, and ignores
`client/target`. Existing projects can opt in by setting `build.client_dir` to a
Cargo package with a `cdylib` target:

```toml
[build]
client_dir = "client"
```

`rusty build` then runs `cargo metadata`, compiles the package for
`wasm32-unknown-unknown`, and runs `wasm-bindgen --target web`. Output goes to
`target/rusty/client`, and `target/rusty/client-manifest.json` records the
entrypoint plus every artifact's path, size, and content fingerprint. The
manifest is validated before it is written: paths must be relative and unique,
the entrypoint must be listed, and fingerprints must be valid. The standalone
deployment bundle in `target/rusty/dist` includes the client and a
runtime configuration that serves it at `/_rusty/client/`. The
`wasm-bindgen` CLI must be installed; set `RUSTY_WASM_BINDGEN` to use a different
executable. `rusty dev` builds the client at startup and rebuilds it when client
sources, Cargo manifests, or `rusty.toml` change.

`serve()` and `serve_with()` expose the generated directory at
`/_rusty/client/`, for example `/_rusty/client/demo_client.js`. The prefix is
reserved for generated artifacts. Custom routers built from `router()` can opt
in with `.client_dir(...)`. `RUSTY_HOST` and `RUSTY_PORT` override the configured
dev bind address.

### Deployment bundle

`rusty build` also writes `target/rusty/dist`, a standalone deployment bundle:

```text
target/rusty/dist/
├── <app executable>
├── rusty.toml
├── deployment.json
├── app-manifest.json
├── client-manifest.json
├── public/
└── client/
```

The bundle validates `client-manifest.json` against the copied client artifacts
and records `clientUrlPrefix` and `clientEntrypoint` in `deployment.json`. The
runtime configuration uses relative paths, so running the executable from the
bundle directory serves `public/` and the generated client without editing the
original project config. Platform-specific adapters remain future work.

## License

MIT OR Apache-2.0

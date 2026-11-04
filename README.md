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
- [x] Public static asset serving, validated build-time manifests, and conditional caching
- [x] Gzip compression for complete dynamic and static responses
- [x] File-based application middleware in `src/middleware.rs`
- [x] Inherited document metadata from `src/app/metadata.rs`
- [x] External CSS preprocessing for PostCSS, Tailwind CSS, Sass, and similar tools
- [x] WebAssembly client builds and automatic client asset serving
- [x] Standalone deployment bundles with app, public, and client artifacts
- [x] Async page/API handlers and chunked streaming SSR
- [x] Loading boundaries and same-origin client navigation
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
├── metadata.rs
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

Metadata modules export `pub fn metadata(context: &mut RequestContext) ->
Metadata` and are inherited from the root to the nearest module. Build metadata
with `Metadata::new()` and chain `with_title`, `with_description`,
`with_canonical`, `with_name`, or `with_property`. Nearer modules override
scalar values and same-key meta tags. The generated title, description,
canonical link, and matching meta tags replace static versions in layout
documents; pages without layouts receive a generated document head.

Application middleware lives in `src/middleware.rs` and exports:

```rust
pub fn middleware(context: &mut RequestContext) -> Result<Option<Response>, ServerError>
```

It runs before pages, API routes, and static files. Return `Some(response)` to
short-circuit the request, or `None` to continue. The generated module enables
file-based discovery; custom routers can call `Router::middleware(...)` directly.

Every application must include a build script that emits
`cargo:rerun-if-changed` for `src` so Cargo re-expands the macro when routes
are added, removed, or changed. `rusty new` generates one; `rusty new --client`
also scaffolds a WebAssembly client package and its layout bootstrap.

The development supervisor runs the application with `cargo run`, sets
`RUSTY_DEV=1`, and restarts it when `.rs`, `Cargo.toml`, or `rusty.toml` files
change. `rusty build` writes the application manifest and delegates native
compilation to Cargo; `rusty start` runs it. Static responses use MIME types
derived from file extensions, `Cache-Control: public, max-age=3600` in production,
and `no-cache` in development. Static and generated client assets also receive
content-based `ETag` and `Last-Modified` headers, and requests with matching
`If-None-Match` or `If-Modified-Since` receive `304 Not Modified` without a body.
Complete text, JSON, JavaScript, SVG, XML, and WebAssembly responses larger than
1 KiB are gzip-compressed when the client accepts gzip. Compressed responses add
`Vary: Accept-Encoding` and use weak ETags. Streaming SSR responses pass through
uncompressed. Application routers enable compression by default; disable it with
`Router::compression(false)` or compose `compression()` into a custom `HttpRouter`.
Async handlers, chunked SSR, loading boundaries, and same-origin client
navigation are supported; platform-specific deployment adapters remain a future
milestone.

## Documentation site

[`examples/site`](examples/site) is a static Rusty site with file-based pages,
a nested docs layout, a dynamic `[slug]` route, a loading boundary, client
navigation, and a generated 404 page. Build it locally with:

```bash
RUSTY_BASE_PATH=/rusty SITE_OUT_DIR=.pages cargo run --release -p site
```

The exported directory is self-contained: the home page is written as
`index.html`, every other route is written as both `docs.html` and a
`docs/index.html` compatibility copy, public assets are copied, and `.nojekyll`
is added. The export also writes `sitemap.xml` and `robots.txt` from the route
table. Links use extensionless URLs, so GitHub Pages serves `/rusty/docs`
without a trailing-slash redirect. The `Deploy site` workflow runs the same
command with the base path reported by `actions/configure-pages`, uploads
`.pages`, deploys it to GitHub Pages, and smoke-tests the live routes.

## Continuous integration

GitHub Actions runs formatting, Clippy, the workspace tests, rustdoc with
warnings denied, and a WASM check for the `rusty` crate test suite. A separate
`Deploy site` workflow exports `examples/site` and publishes it to GitHub Pages
on every push to `main`.

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

# [build.css]
# command = "npx tailwindcss -i ./styles/app.css -o ./public/assets/app.css"
# inputs = ["styles", "tailwind.config.js"]

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

### CSS preprocessing

Rusty serves and bundles CSS but does not parse or transform it. Configure an
external command for PostCSS, Tailwind CSS, Sass, or any other preprocessor:

```toml
[build.css]
command = "npx tailwindcss -i ./styles/app.css -o ./public/assets/app.css"
inputs = ["styles", "tailwind.config.js"]
```

The command runs with the project root as its working directory. `rusty build`
runs it before analyzing the asset manifest and compiling, so generated files
under `public` are served and included in the deployment bundle. `rusty dev`
runs it at startup and again when a file under `build.css.inputs` or
`rusty.toml` changes. When `inputs` is empty, Rusty watches common stylesheet
extensions (`.css`, `.scss`, `.sass`, `.less`, `.pcss`) plus `package.json`,
Tailwind, and PostCSS configuration files. List custom tool configuration files
in `inputs` if they use another name.

Commands run through `sh -c` on Unix and `cmd /C` on Windows, so quoted
arguments, environment variables, and local wrappers such as `npx` work as they
do in a terminal. The tools themselves must already be installed. Preprocessing
errors fail `rusty build` and are reported but do not stop the `rusty dev`
watcher. `rusty check` and `rusty generate` only analyze Rust sources and do not
run preprocessing commands.

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

### Async handlers and streaming SSR

Page and API modules may export `async fn` handlers. `#[rusty::app]` detects the
`async` keyword at compile time and registers a future-backed handler. The
blocking server runs each future on its connection thread with
`rusty_server::block_on`, so this adds no runtime dependency or thread pool.
`PageRoute::async_new` and `ApiHandler::Async` are available for hand-built
routers.

Pages are serialized into ordered eight-kilobyte chunks and sent with
`transfer-encoding: chunked`. API routes and middleware can opt in with
`Response::stream` or `Response::stream_html`. `Response::body()` collects a
streaming body when a transport-neutral caller or test needs the full bytes.
Chunked SSR currently starts after the page future completes; it does not yet
insert loading boundaries or await suspended subtrees.

### Loading boundaries and client navigation

A `loading.rs` module exports `pub fn loading() -> Element`. The nearest loading
boundary is attached to each page during routing. When any page has a loading
boundary, the generated runtime enables same-origin client navigation
automatically; `Router::navigation(true)` opts in without boundaries and
`Router::navigation(false)` restores normal browser navigation.

Clicking an eligible same-origin link shows the target route's loading boundary
in a fixed overlay, fetches the destination, and replaces the document with the
response instead of waiting for a blank browser transition. The URL is updated
with `history.pushState`; back and forward use the same path. The loading
boundary is rendered at router-build time, so it is synchronous and does not
receive a `RequestContext`.

This is whole-document navigation, not a persistent single-page shell:
scripts in the fetched document run again, and client state is recreated. Links
with `target`, `download`, `rel="external"`, or `data-rusty-reload` keep native
behavior, as do cross-origin links, hash-only links, and non-HTML responses.
Forms and links to reserved `/_rusty/` paths are not intercepted. If a target
route has no loading boundary, a generic status overlay is shown.

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

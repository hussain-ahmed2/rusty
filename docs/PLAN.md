# Rusty — Implementation Plan

> The Next.js experience. The Rust language.

Rusty is a Rust-native, full-stack web framework that preserves the parts of the
Next.js mental model developers rely on while owning its component system,
reactivity engine, renderer, router, compiler, and tooling.

This document is the source of truth for the build order, scope, and engineering
rules. A milestone is only complete when its acceptance tests pass and the
workspace compiles without warnings.

## Compatibility contract

These concepts are intentionally kept close to Next.js:

| Next.js | Rusty |
| --- | --- |
| `src/app/page.tsx` | `src/app/page.rs` |
| `src/app/layout.tsx` | `src/app/layout.rs` |
| `src/app/loading.tsx` | `src/app/loading.rs` |
| `src/app/error.tsx` | `src/app/error.rs` |
| `src/app/not-found.tsx` | `src/app/not_found.rs` |
| `src/app/api/*/route.ts` | `src/app/api/*/route.rs` |
| `"use server"` | `#[server]` |
| `"use client"` | `#[client]` |
| `src/middleware.ts` | `src/middleware.rs` |
| Server Components | Server-rendered Rust components |
| Server Actions | Typed Rust server functions |
| JSX | `view! { ... }` |
| React state/hooks | Fine-grained Rust signals |

Internals will differ where Rust's type system, ownership model, compilation
model, and WebAssembly target require it. Syntax and project structure should
remain immediately recognizable.

## Engineering rules

- **Safe by default.** No `unsafe` code. Every crate uses `#![forbid(unsafe_code)]`.
- **Typed boundaries.** Framework APIs use newtypes and validated invariants
  instead of unchecked strings where practical.
- **Safe rendering.** Text and attribute values are escaped by default. Raw HTML
  requires an explicit API.
- **Small dependency surface.** Foundational crates only when they materially
  reduce risk. No wrapper around another full-stack framework.
- **Honest scope.** Features are reported as complete only when implemented,
  documented, and tested.
- **Incremental delivery.** Every milestone leaves the workspace compiling, the
  example application working, and all tests passing.
- **Documented public APIs.** Public items carry Rustdoc. Architectural choices
  are recorded here.
- **Clean crate boundaries.** Dependencies flow one direction. No cycles.
- **Verification before completion.** `cargo fmt --check`, `cargo check
  --workspace --all-targets`, `cargo clippy --workspace --all-targets
  -- -D warnings`, and `cargo test --workspace` must pass.

## Crate map (introduced over time)

| Crate | Responsibility | Introduced |
| --- | --- | --- |
| `rusty` | Public facade and prelude | M1 |
| `rusty-core` | Element model, components, children, conversions | M1, M2 |
| `rusty-macros` | `view!`, component, server, and application macros | M1 (`view!`), M2 (`#[component]`), M8 (`#[rusty::app]`) |
| `rusty-ssr` | Server-side HTML rendering | M1 |
| `rusty-reactivity` | Signals, memos, effects, scheduling | M3 |
| `rusty-vdom` | Virtual node snapshots for client rendering | M4 |
| `rusty-dom` | Browser DOM abstraction | M4 |
| `rusty-renderer` | Client mounting, patching, and reactive roots | M4 |
| `rusty-hydrate` | Serialization protocol and hydration | M5 |
| `rusty-router` | File-based routing and route manifests | M6 |
| `rusty-server` | HTTP runtime, middleware, request context | M7 |
| `rusty-rpc` | Typed server functions | M7 |
| `rusty` app runtime | Route metadata and document-head generation | M9 |
| `rusty-compiler` | Application analysis and code generation | M8 |
| `rusty-build` | Native/WASM build orchestration (planned) | M8 |
| `rusty-cli` | `new`, `check`, `generate`, `dev`, `build`, and `start` | M8 |
| `rusty-config` | `rusty.toml` parsing and validation | M8 |

## Milestones

**Status:** Milestone 8 in progress. Configuration, application analysis, the
CLI commands, compiler-generated runtime registration, page/API/RPC dispatch,
development restart-on-change, native build/start, static public assets,
build-time asset manifests, WebAssembly client builds, generated client
serving, async handlers, chunked streaming SSR, loading boundaries, and
same-origin client navigation are complete. Platform-specific deployment
adapters remain.

### Milestone 1 — Foundation (complete)

Deliverables:

- Cargo workspace with `rusty`, `rusty-core`, `rusty-macros`, and `rusty-ssr`.
- Validated `TagName` and `AttributeName` newtypes.
- `Element` model for HTML elements, text, fragments, and explicit raw HTML.
- `view!` procedural macro for:
  - nested HTML elements,
  - string-literal text nodes,
  - static string attributes,
  - boolean attributes,
  - self-closing and void elements,
  - fragments (multiple root nodes),
  - custom element names containing `-`.
- Safe HTML serialization with escaping and void-element handling.
- `rusty::prelude`.
- Working `examples/hello-world` application.
- Unit and integration tests, including the acceptance test.

Explicit non-goals for M1: components, props, children, dynamic expressions,
conditionals, iteration, events, reactivity, browser runtime, hydration, routing,
server functions, API routes, compiler, CLI, and configuration files.

Deferred syntax produces clear compiler errors rather than silent or
half-working behavior. Component tags (uppercase first letter) and dynamic
`{ ... }` nodes are rejected with a message naming the milestone that introduces
them.

### Milestone 2 — Component system (complete)

Deliverables:

- `#[component]` attribute macro.
- Typed props generated from function parameters.
- `Children` for nested content.
- Dynamic text and attribute expressions.
- `{#if}` / `{:else if}` / `{:else}` conditionals.
- `{#each}` iteration with destructuring.
- Component composition.
- `IntoElement` and `IntoAttributeValue` conversion traits.
- Unit, integration, example, and doctest coverage.

Explicit non-goals for M2: reactive updates, event handlers, generic or async
components, browser rendering, hydration, and lifecycle hooks.

### Milestone 3 — Reactivity (complete)

Deliverables:

- `Signal<T>` for reactive state.
- `Memo<T>` for lazy derived values with cached recomputation.
- `Effect` for tracked side effects with cleanup and disposal.
- Automatic dependency tracking with dynamic dependency sets.
- `batch` for coalescing notifications and `untrack` for untracked reads.
- An owner tree with weak parent links and strong child ownership, providing
  cycle-safe lifecycle cleanup.
- Context propagation through owner scopes.
- Reactive values render their current value in `view!` and HTML attributes.

Explicit non-goals for M3: browser DOM patching, event handlers, hydration,
async effects, and multi-threaded reactive state. Reactive values are updated in
memory; DOM updates arrive in M4.

### Milestone 4 — Browser runtime (complete)

Deliverables:

- `wasm32-unknown-unknown` support for the core crates, renderer, facade, and
  browser example.
- A `Dom` trait covering element, text, raw HTML, attribute, child, and event
  operations.
- `MockDom` for deterministic native tests and `WebDom` backed by `web-sys`.
- `rusty-vdom` snapshots converted from the owned `Element` tree.
- A renderer that mounts a tree and patches attributes, text, raw HTML,
  listeners, and children in place.
- `on:name={handler}` event syntax and browser-independent `Event` values.
- Reactive roots that run views inside effects and keep node identity stable.
- A browser counter example with an `on:click` handler.

Explicit non-goals for M4: hydration, keyed list diffing, event delegation,
event modifiers, async effects, routing, and server functions.

### Milestone 5 — SSR and hydration (complete)

Deliverables:

- Deterministic pre-order marker comments for every flattened DOM node, using
  `HydrationId` values shared by the server renderer and client hydrator.
- Hydration-aware SSR that emits markers around elements, text, and raw HTML,
  and wraps explicit raw HTML in a `<rusty-raw-html>` display-contents container.
- A `HydrationSession` backed by serde and `serde_json`, with stable signal keys,
  server-side state capture, client-side restoration, and a `<script>` state
  payload that escapes `<` in serialized JSON.
- Client hydration that walks markers, validates tags, attributes, text, and raw
  HTML, attaches event listeners, and adopts existing nodes in place.
- Mismatch reporting with the failing node id and a recovery path that clears
  the container and performs a fresh mount.
- Facade hydration helpers for a container id and the document body, plus a
  hydrated browser counter example.

Explicit non-goals for M5: streaming SSR, partial hydration, per-component
islands, async data loading, and compiler-generated hydration boundaries.

### Milestone 6 — Routing (complete)

Deliverables:

- `RoutePattern` and `RouteSegment` types for static, dynamic, and catch-all
  URL segments, with validation for catch-all placement and parameter names.
- `src/app` file discovery for `page.rs`, nested `layout.rs` chains, and nearest
  `loading.rs`, `error.rs`, and `not_found.rs` boundaries.
- Route groups written as `(name)`, which keep layouts without adding a URL
  segment, and private directories beginning with `_` or `.`.
- A `RouteManifest` with deterministic ordering, most-specific matching, typed
  captured parameters, root not-found fallback, and duplicate-route detection.
- JSON serialization and validated restoration for build-time route manifests.
- A native routing example that discovers a real `src/app` tree and matches a
  dynamic request.

Explicit non-goals for M6: route handlers, API routes, middleware, links,
client-side navigation, route prefetching, parallel routes, intercepting routes,
optional catch-all segments, and compiler-generated entry points.

### Milestone 7 — Full stack (complete)

Deliverables:

- Transport-neutral HTTP types: methods, status codes, validated multi-value
  headers, requests, and responses.
- Method-aware `HttpRouter` with route parameters, middleware, fallbacks, and
  `404`/`405` handling.
- `RequestContext` with typed extensions, captured parameters, and an auth
  identity.
- Bearer authentication middleware plus authentication traits for application
  authenticators.
- Double-submit-cookie CSRF middleware with generated tokens and secure cookie
  options.
- `#[server]` attribute macro, generated marker types, typed client stubs, native
  invokers, and an RPC registry.
- JSON server-function protocol with default and custom paths.
- `route.rs` API endpoint discovery and matching alongside pages.
- Native full-stack example covering routing, auth, CSRF, and RPC.

Explicit non-goals for M7: TCP/HTTP transport adapters, async handlers,
WebSocket subscriptions, compression, streaming, rate limiting, database
integrations, and compiler-generated registration.

### Milestone 8 — Compiler and tooling (in progress)

Deliverables completed:

- Optional `rusty.toml` with typed `[app]`, `[build]`, and `[dev]` sections,
  defaults, unknown-field rejection, and relative-path validation.
- `AppCompiler`, which combines page/API route discovery with source analysis
  and produces a versioned `AppManifest`.
- `#[server]` discovery over Rust source using `syn`, including default paths,
  literal and `path = "..."` forms, module-path inference, and duplicate-path
  rejection.
- Validated manifest JSON serialization, disk writes, and restoration.
- `rusty new`, `rusty check`, and `rusty generate`, with project scaffolding,
  non-empty-directory protection, and a local facade dependency.
- A native tooling example that analyzes its own routes and server functions.
- A blocking HTTP/1.1 TCP transport with bounded header/body sizes, timeouts,
  scoped connection threads, and safe handler-error responses.
- A `#[rusty::app]` attribute macro that scans the application directory at
  compile time, declares a module for each route file, and generates a route
  registry.
- Page, layout, error-boundary, not-found, API-method, and `#[server]` RPC
  registration contracts.
- File-based `src/middleware.rs` discovery with synchronous short-circuit or
  continue semantics before routes and static assets.
- File-based `src/app/metadata.rs` discovery with inherited titles, descriptions,
  canonical links, and named/property meta tags that replace static layout tags.
- Native `Router` dispatch for pages, API routes, RPC functions, and development
  diagnostics.
- `rusty dev` with rebuild-and-restart file watching, `RUSTY_DEV`, `RUSTY_HOST`,
  and `RUSTY_PORT` integration, including WASM client rebuilds.
- `rusty build` and `rusty start` native Cargo orchestration.
- Safe `public/` static serving with MIME types, cache policy, traversal
  rejection, and root-relative index files.
- Validated static asset entries with content fingerprints in the versioned
  application manifest.
- WebAssembly client builds through Cargo and `wasm-bindgen`, with a validated,
  fingerprinted client artifact manifest.
- Runtime serving of generated client artifacts under `/_rusty/client/`.
- Standalone deployment bundles with copied native, public, client, and manifest
  artifacts plus a relative runtime configuration.
- Recursive build-script tracking so route additions and removals re-expand the
  application macro.
- Root-layout application to not-found pages, with fallback document generation
  when no layouts exist.
- A generated-runtime example application.

Remaining for Milestone 8:

- Platform-specific deployment adapters and incremental builds for larger
  applications.
- Dependent server/client analysis.

Explicit non-goals for the current slice: database integrations and deployment
adapters.

### Milestone 9 — Production features

Static generation, metadata, caching, image handling, and deployment support.
Static assets emit content-based `ETag` and `Last-Modified` headers and support
`If-None-Match` and `If-Modified-Since` conditional requests. Complete text,
JSON, JavaScript, SVG, XML, and WebAssembly responses larger than 1 KiB are
gzip-compressed when negotiated, with `Vary: Accept-Encoding` and weak ETags.
Streaming responses are compressed as their chunks are produced. Route metadata from
`src/app/metadata.rs` is inherited and replaces static layout head tags. Static
generation, image handling, and broader deployment support remain.
The documentation website is generated from `examples/site` and deployed through
GitHub Pages.

## Milestone 1 architecture

### Crate boundaries

```text
rusty-macros ──> rusty-core
      │
      └─────────> (generated code uses ::rusty::__private)

rusty ──> rusty-core
      ├──> rusty-ssr ──> rusty-core
      └──> rusty-macros
```

`rusty-macros` depends on `rusty-core` for shared validation helpers such as the
void-element table and on `rusty-compiler` for `#[rusty::app]` analysis. Generated
code still references only the public `rusty` facade. Generated code does not reference `rusty-core` directly,
so applications only need the `rusty` facade.

### Element model

`Element` is a closed, owned tree:

- `Element::Html(HtmlElement)` — a validated tag, attributes, and children.
- `Element::Text(String)` — escaped when rendered.
- `Element::Fragment(Vec<Element>)` — ordered roots with no wrapper element.
- `Element::RawHtml(String)` — explicit, documented escaping bypass.

`HtmlElement` can only be constructed with validated names. `TagName` and
`AttributeName` enforce character sets at construction, so rendering cannot emit
attacker-controlled markup names. Attribute values are typed as text or boolean;
`false` booleans are omitted.

The tree is intentionally materialized in M1 and M2. This keeps SSR simple and
makes the macros independently testable. Reactivity in M3 will add node-level
update handles without retrofitting unsafe shared mutable state.

### `view!` macro

The macro is a hand-written recursive descent parser over `proc_macro2` tokens,
using `syn` for Rust literals, identifiers, spans, and error reporting.

Generated code uses a small hidden helper surface in `rusty::__private` so the
output stays stable and the facade can change internally without breaking
expansions. Tag and attribute names are emitted as `'static` string literals;
the helpers validate them defensively before constructing the tree.

Compile-time diagnostics include:

- unmatched or mismatched closing tags,
- unsupported node forms,
- unknown control directives,
- mismatched control markers,
- duplicate attributes and duplicate children sources.

### SSR serializer

`render_to_string` writes into a `String` and returns a typed `RenderError` for
structural violations such as children on a void element. Escaping is mandatory:

- text: `&`, `<`, `>`.
- attribute values: `&`, `<`, `>`, `"`, `'`.

Raw HTML is never escaped and is only reachable through the explicit
`Element::raw_html` constructor. The serializer does not trust tag/attribute
strings: even though the newtypes validate them, the writer uses the validated
values only.

### Acceptance test

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

Expected output:

```html
<main><h1>Hello from Rusty</h1><p>A Next.js-inspired framework built with Rust.</p></main>
```

## Milestone 2 architecture

### Component model

`#[component]` transforms a function such as:

```rust
#[component]
pub fn Greeting(name: String) -> Element {
    view! { <p>{format!("Hello, {name}!")}</p> }
}
```

into a braced struct plus a `Component` implementation:

```rust
pub struct Greeting {
    pub name: String,
}

impl Component for Greeting {
    fn render(self) -> Element {
        let Self { name } = self;
        // original function body
    }
}
```

The component tag `<Greeting name="Rusty" />` becomes a normal struct literal:

```rust
render_component(Greeting {
    name: Into::into("Rusty"),
})
```

This design gives compile-time checking for missing props, unknown props, and
property types without a second props layer or procedural-macro-only validation.
Component names must start with an uppercase ASCII letter so the parser can
distinguish them from HTML tags.

Limitations are explicit: generic, async, and `self`-taking components are
rejected with milestone-specific diagnostics. Props should use owned or
`'static` types because references with elided lifetimes require generic
component support.

### Children

Nested component content is built as one element expression and wrapped in
`Children`. A component opts in by declaring a `children` parameter:

```rust
#[component]
pub fn Layout(children: Children) -> Element {
    view! { <main>{children}</main> }
}
```

A `children: Element` parameter also works through `From<Children> for Element`.
The syntax is intentionally explicit:

- `<Card />` is self-closing and passes no `children` field. Components without
  children must use this form.
- `<Card>...</Card>` is paired and always passes a `children` field, even when
  the body is empty.
- `<Card children={content} />` passes programmatic children on a self-closing
  tag.
- A paired tag cannot also declare a `children` prop; that is a macro error.

This keeps props exhaustive at compile time without inventing optional props.

### Dynamic conversions

- Dynamic nodes and component props use `IntoElement`.
- Dynamic HTML attribute values use `IntoAttributeValue`.
- `Option<T>` renders nothing when `None`.
- `false` booleans and `AttributeValue::None` omit attributes.
- `Vec<T>` and arrays render as fragments.
- Iteration renders through a `FnMut` closure and preserves ownership.

### Control flow

Control blocks are parsed as distinct `view!` nodes, not Rust expressions:

- `{#if condition} ... {:else if condition} ... {:else} ... {/if}`
- `{#each iterable as pattern} ... {/each}`

At code generation time they become ordinary Rust `if` and iterator expressions.
Conditions and iterables are fully type-checked Rust expressions. Branches that
contain multiple nodes are wrapped in fragments.

The `#each` iterable ends at the first top-level `as` token. Wrap cast
expressions in parentheses so they are not mistaken for the pattern separator.

### Compile-time safety

Macro expansion rejects:

- lowercase component function names,
- component tags with `-` or `:` separators,
- component props that are not Rust identifiers,
- duplicate props or HTML attributes,
- `children` supplied twice,
- unknown control directives,
- mismatched or unclosed control blocks,
- complex component parameter patterns,
- generic, async, const, unsafe, extern, or variadic components,
- where clauses without generic support.

Function attributes are preserved. `#[cfg]` and `#[cfg_attr]` apply to both the
generated struct and its `Component` impl, while `#[inline]`, `#[cold]`,
`#[track_caller]`, and lint attributes also apply to the generated `render`
method.

## Milestone 3 architecture

### Runtime and ownership

The reactivity runtime is thread-local. Signals are `Rc`-backed and intentionally
not `Send` or `Sync`; all reactive work runs on the thread that created the
value. This keeps dependency tracking and scheduling deterministic without locks
or `unsafe` code.

Owners form a disposal tree:

- an owner holds strong references to its children and disposables,
- each child holds a weak reference to its parent,
- dropping or disposing an owner recursively disposes children, effects, and
  cleanups,
- dropping a child detaches it from its parent.

Weak parent links are what make the ownership graph cycle-safe. Effects are
owned by the current owner and can also be disposed early through their handle.
Effects created outside an explicit `create_root` attach to a thread-local root
that lives for the rest of the thread.

### Signals, memos, and effects

`Signal<T>` stores a value and a set of weakly held subscribers. Reads record the
current observer as a dependency; writes notify subscribers. `set` always
notifies, while `set_if_changed` and `update` suppress notifications when the
value is unchanged.

`Memo<T>` is a lazy derived value. It records its own dependencies, caches its
value, and becomes dirty when a dependency changes. Recomputation happens on the
next read. Re-entering a memo while it is computing is a dependency cycle and
panics with a clear diagnostic.

`Effect` runs immediately, clears its previous dependency set before each run,
and reruns when a dependency changes. Effects can register cleanup with
`on_cleanup`; cleanups run before reruns and on disposal. A callback that
unconditionally writes a signal it reads intentionally schedules another run and
must be avoided.

### Scheduling and batching

Writes enqueue affected effects in a runtime queue. Queued effects are
deduplicated by runtime-assigned subscriber identity and run in FIFO scheduling
order. `batch` defers effect execution until the outermost batch completes.
Effects created or triggered during a run are queued safely instead of being
executed reentrantly. Disposing an effect cancels its queued work.

`untrack` suppresses dependency recording for the current computation depth while
still allowing nested memos and effects to track their own reads. Writes made
inside `untrack` are flushed after the untracked scope ends, so rerun effects
record dependencies normally. Borrowed reads through `Signal::with` and
`Memo::with` likewise defer effect flushing until the borrow ends.

### Rendering integration

`Signal<T>` and `Memo<T>` implement `IntoElement` and `IntoAttributeValue` when
`T` can be rendered. `view!` displays the current value during rendering. The M4
client runtime runs the view function inside an effect, so subscriptions become
live when the tree is mounted.

Limitations are explicit: reactive state is single-threaded, effects are
synchronous, and there is no batching timer or scheduler abstraction yet.

## Milestone 4 architecture

### Event model

Events are a distinct part of an HTML element, not attributes:

```rust
view! {
    <button on:click={move |_| count.set(count.get() + 1)}>
        {count}
    </button>
}
```

The parser strips the `on:` prefix and validates the remaining name with
`EventName`. Event names must start with an ASCII letter and may contain only
letters, digits, `-`, and `_`. Handlers must be `{ ... }` expressions and are
stored as `EventListener` values containing `Rc<dyn Fn(Event)>`.

Listeners live in a separate `HtmlElement::listeners` field. Server rendering
therefore ignores them automatically: server HTML contains no event attributes
or handler code. Component props cannot use `on:click` because component property
names must be Rust identifiers; components that need callback props should use a
normal prop such as `on_click`.

`Event` is browser-independent and currently exposes the event name, target
value, key, checked state, and bubbling. `MockDom` dispatches events to matching
listeners and bubbles to ancestors. `WebDom` converts `web-sys` events and adds
the listener through `add_event_listener_with_callback`.

### Virtual nodes

`VNode` is an owned snapshot of `Element` with element, text, raw HTML, and
fragment variants. Conversion validates that void elements have no children, so
the renderer can rely on the same invariant as SSR.

Fragments are retained in the snapshot but flattened before mounting so they
never create a DOM node. This keeps the representation close to `Element` while
allowing fragments at the root and inside element children.

### DOM abstraction

`Dom` has an associated `Node` handle and the minimum operations the renderer
needs:

- create elements, text, and trusted raw HTML wrappers,
- append, insert, remove, and replace children,
- set and remove attributes,
- set text and raw HTML,
- set and remove event listeners.

`MockDom` stores a parent-aware node arena and serializes back to HTML for
assertions. `WebDom` wraps `web-sys`; raw HTML uses a
`<rusty-raw-html style="display: contents">` wrapper so it can be patched without
injecting markup into the surrounding element.

### Renderer and patching

`Renderer<D: Dom>` owns a DOM and a vector of mounted roots. The first render
mounts the flattened tree. Later renders compare children by position:

- matching element tags patch attributes, listeners, and children in place,
- matching text nodes patch only changed text,
- matching raw HTML nodes patch only changed markup,
- changed node kinds are replaced,
- extra children are removed and missing children are appended.

Attribute diffing applies only changed values and removes attributes absent from
the next tree. Listener diffing removes names that disappeared and replaces
handlers when the `EventListener` identity changes. Nodes that keep the same kind
and position preserve their DOM identity; there is no keyed diffing yet.

### Reactive roots

`ReactiveRoot<D>` owns a root `Owner` and an `Rc<RefCell<Renderer<D>>>`. Its view
function runs inside `create_effect`, so signals read by the view subscribe the
effect. A signal write reruns the view, rebuilds the element tree, and patches the
mounted DOM.

Dropping `ReactiveRoot` disposes the owner, which disposes the render effect, and
drops the renderer, which removes mounted nodes from the container. On
WebAssembly, `rusty::client::mount_to_body` connects `WebDom` to the document
body and returns the root; callers must keep that root alive.

The view closure is rerun after every reactive change, so state that must survive
rerenders belongs outside the closure. Signals created inside the view are
recreated at their initial value on each run.

The renderer and runtime are single-threaded and synchronous. `with_dom` and
`with_renderer` borrow the renderer immutably and must not be called from inside
an in-progress render.

## Milestone 7 architecture

### HTTP runtime

HTTP values are transport-neutral owned types. Header names are normalized to
ASCII lowercase, names must be valid tokens, and values reject CR, LF, and NUL to
prevent response splitting. `Request` exposes the raw target, path, query string,
and undecoded query parameters. `Response` provides text, HTML, and JSON helpers
with explicit content types.

`RequestContext` pairs a request with captured route parameters, a typed
extension map, and an optional `AuthIdentity`. Extensions are keyed by `TypeId`
and require `Send + Sync`, so middleware can hand typed values to handlers
without string keys.

### Routing and middleware

`HttpRouter` registers any-method or method-specific handlers. Route patterns use
the M6 matcher, and the most specific matching pattern wins. Duplicate URL shapes
for overlapping methods are rejected at registration. A path that matches but
has no handler for the request method returns `405`; a missing path uses the
custom fallback or `404`.

Middleware is an ordered chain of `Fn(&mut RequestContext, Next)`. Each layer may
inspect or mutate the context, short-circuit with a response, or call
`Next::run`. Authentication middleware attaches an identity; requiring middleware
returns `401` when credentials are absent.

### Authentication and CSRF

`Authenticator` resolves an optional identity from a request. The provided bearer
authenticator stores token-to-identity mappings and compares tokens in constant
time. Applications can implement the trait for database-backed sessions.

CSRF uses the double-submit-cookie pattern. Safe methods issue a 256-bit random
token cookie when absent and expose the token through `CsrfToken` in extensions.
Unsafe methods require the cookie and `x-csrf-token` header to match in constant
time. The cookie is intentionally not `HttpOnly`, is `SameSite=Lax`, and can be
marked `Secure`.

### Server functions

`#[server]` accepts a synchronous, non-generic free function returning
`Result<T, RpcError>`. Arguments must be simple identifiers and serialize as a
tuple. The macro emits:

- the original function on native targets,
- a WebAssembly client stub with the same signature that invokes the generated
  marker through the installed `Transport`,
- a hidden marker implementing `ServerFunction` with the path, argument tuple,
  and output type,
- a native `ServerInvoker` that decodes JSON arguments, calls the function, and
  encodes the result.

Default paths are `/_rusty/rpc/<name>`; a literal or `path = "..."` overrides
them. `RpcRegistry` registers markers by path and exposes JSON invocation plus an
HTTP handler. Clients install a thread-local transport with `set_transport` or
use `RpcClient`/`invoke_with` with an explicit transport. Client stubs propagate
the original `RpcError`.

### API routes

`RouteManifest` records `route.rs` files as `ApiRoute` entries using the same
directory conventions and patterns as pages. A page and API route cannot share a
URL shape. `match_api_path` returns the endpoint and captured parameters, letting
the compiler or an application adapter bind endpoints to handlers.

## Definition of done for each milestone

1. Scope implemented with no placeholders.
2. Public APIs documented.
3. Unit tests cover core behavior and failure cases.
4. Integration/example tests cover the developer-facing path.
5. Formatter, compiler, Clippy, and tests pass.
6. Plan and README status updated.
7. Remaining limitations stated explicitly.

## Milestone 5 architecture

### Hydration markers

The server renderer flattens fragments, then writes one comment marker before
every DOM node in pre-order:

```html
<!--rusty:0--><main><!--rusty:1--><p><!--rusty:2-->Hello</p></main>
```

Markers use comment content rather than elements so they are invisible and
cannot affect CSS layout. The client hydrator consumes markers in the same
pre-order, removes each one, and uses the next sibling as the candidate node.
This keeps identities deterministic without requiring a component graph or
serialized tree.

### State protocol

Application state is shared through `HydrationSession`. Each signal is created
with a stable key and a default:

```rust
let session = HydrationSession::server();
let count = session.signal("count", 0_u32)?;
```

Server sessions record current signal values when `state_json` or `state_script`
is called. Client sessions decode the same JSON and replace defaults with
restored values. Duplicate keys are rejected so state ownership stays explicit.
Serialized JSON escapes `<` to prevent a value from closing the state script.

The state script is `<script id="__RUSTY_STATE__" type="application/json">` and
must sit outside the hydrated container. The bootstrap script can read it with
`HydrationSession::from_document`.

### Adoption and recovery

Hydration validates node kind, tag name, normalized attributes, text, and raw
HTML. Matching nodes are adopted and event listeners are attached directly to
them. On the first mismatch, the renderer removes the marker already consumed,
clears the container, mounts the client tree fresh, and returns
`HydrationOutcome::Recovered` with a `HydrationMismatch` describing the expected
and found values. This favors a correct page over a partially adopted tree.

Handlers are stored separately from attributes, so server rendering never emits
event wiring. Client hydration attaches listeners only after the corresponding
element has matched.

## Milestone 6 architecture

### Route patterns

Routes are represented as ordered segments:

- `Static("blog")` matches a literal segment.
- `Dynamic("slug")` matches one segment and captures it as `slug`.
- `CatchAll("path")` matches the remaining segments and captures them joined
  with `/`.

Catch-alls must be the final segment and must match at least one segment.
Parameter names are validated and may appear only once in a pattern. Matching
normalizes repeated and trailing slashes and ignores query strings and
fragments; percent-encoded values are captured without decoding.

Static segments are more specific than dynamic segments, which are more
specific than catch-alls, compared from left to right. This allows `/shop/new`,
`/shop/[category]`, and `/shop/[...path]` to coexist with deterministic
precedence.

### Discovery

`RouteManifest::discover` walks the application directory depth-first. Directory
names define URL segments; `(group)` directories are transparent and `_` or `.`
directories are skipped. Each directory may contribute one layout and one of each
boundary, which are inherited by descendants until overridden.

Pages are recorded with their full URL pattern, relative page path, ordered
layout chain, and nearest boundaries. Duplicate URL shapes are rejected even when
they come from different route groups. Paths stored in the manifest are relative
to the application root so the manifest can be relocated as a build artifact.

### Manifests and matching

A manifest serializes to JSON and is validated again when restored. Validation
rechecks segment rules and duplicate route shapes, so a malformed artifact fails
loudly. Route matching scans the manifest and returns the most specific match as
a `RouteMatch` containing the entry and typed parameters.

## Milestone 8 architecture

### Configuration

Configuration is loaded from `<project>/rusty.toml`. A missing file returns the
defaults rather than an error, so convention-driven applications stay
zero-config. TOML documents use `deny_unknown_fields` at every level, which turns
typos into parse errors instead of silently ignored settings.

Paths are validated as non-empty relative paths without `..` components. The
config crate does not touch the filesystem beyond reading the TOML file; the
compiler reports missing application or source directories with their resolved
paths. The development host and port are validated and consumed by `rusty dev`
and the standard application runtime, with `RUSTY_HOST` and `RUSTY_PORT` taking
precedence. `app.public_dir` defaults to `public` and `build.client_dir` is an
optional client package; both are resolved relative to the project root.

### Application analysis

`AppCompiler` joins three independent analyses. `RouteManifest::discover` handles
the `src/app` conventions from Milestone 6. A source walk parses every `.rs` file
under the configured source root with `syn` and records functions carrying a
`#[server]` attribute. A public-directory walk records static assets, their
root-relative URLs, and sizes.

Server function paths accept `#[server]`, `#[server("/path")]`, and
`#[server(path = "/path")]`. Module paths are inferred from the file path
(`src/rpc.rs` becomes `crate::rpc`) and inline modules (`mod nested`) are
appended. Root `lib.rs` and `main.rs` map to `crate`, and `src/bin` is ignored
because it contains separate binary crate roots. Duplicate server paths are
rejected before a manifest is produced.

The compiler performs structural analysis only. It does not type-check function
bodies, analyze native/client dependencies, or generate entry-point code; those
remain later Milestone 8 work.

### HTTP transport

`HttpServer` turns the transport-neutral HTTP types into a working TCP adapter.
It accepts origin-form HTTP/1.0 and HTTP/1.1 requests, requires
`Content-Length` for request bodies, and closes the connection after one
response. Chunked bodies and HTTP/2 are rejected with explicit status codes
rather than partially supported.

The request line and headers are read incrementally with a hard size cap, so a
client cannot force an unbounded allocation before framing is known. Body sizes
are checked before allocation. Read and write timeouts default to 30 seconds and
are configurable through `ServerConfig`.

Connections run on scoped threads without requiring `'static` handlers, which
keeps a router borrowed for the lifetime of the accept loop. Handler errors with
5xx status become a generic `internal server error` response so internal details
do not leak; 4xx authentication and CSRF messages remain visible. A shutdown
handle stops the non-blocking accept loop and waits for active scoped threads.

TLS, keep-alive, chunked transfer encoding, request trailers, and HTTP/2 remain
unimplemented and belong to later transport work.

### File-based application runtime

`#[rusty::app]` runs as a procedural macro while the application compiles. It
uses `AppCompiler` to discover routes and server functions, reads each route
module, and generates a module tree with absolute `#[path]` attributes. The
generated module exports `router()` and `serve()`.

The runtime contracts are:

- page: `fn(&mut RequestContext) -> Result<Element, ServerError>`
- layout: `fn(Children) -> Element`
- error boundary: `fn(&ServerError) -> Element`
- not found: `fn() -> Element`
- API method: `fn(&mut RequestContext) -> Result<Response, ServerError>`
- server function: a `#[server]` marker implementing `ServerInvoker`

Pages render through `rusty-ssr`, apply layouts from root to leaf, and use the
nearest error boundary when a handler fails. Layout-free pages receive a minimal
HTML document; pages with layouts are assumed to render their own document.
Not-found pages use the root layout chain when one exists. API routes register
only exported methods and return `405` for missing methods. Server functions are
registered into `RpcRegistry` and mounted at their declared paths.

### Static assets and manifests

`public` files are served after application routes and before the not-found
boundary. Request paths are percent-decoded, split into segments, rejected when
they contain `.` or `..` segments, backslashes, or drive separators, then joined
to a canonicalized public root. Canonicalization plus a prefix check prevents
symlinks from escaping the directory. Directory requests map to `index.html`.

Responses set a MIME type from the extension, `X-Content-Type-Options: nosniff`,
and a cache policy: `public, max-age=3600` normally and `no-cache` when
diagnostics are enabled. Only `GET` and `HEAD` are served; other methods fall
through to the application not-found boundary.

The compiler walks `public` recursively without following symlinks. Asset URLs
percent-encode path segments, and the manifest rejects empty paths, parent
components, malformed URLs, and duplicate URLs. Each asset also records a stable
FNV-1a content fingerprint for deployment cache keys. Missing public directories
produce an empty asset list. `rusty build` analyzes and writes this manifest before
Cargo compiles the application. Static responses currently buffer the whole file;
streaming bodies remain future work.

### WebAssembly client builds

`rusty new <path> --client` scaffolds a separate `client` package with a
`cdylib` target, sets `build.client_dir`, injects an `init()` module bootstrap
into the root layout, and ignores `client/target`. Existing projects can opt in
by setting `build.client_dir`.

When `build.client_dir` is set, `rusty build` runs `cargo metadata` against that
package, requires a `cdylib` target, and compiles it for
`wasm32-unknown-unknown` with the same debug/release profile as the native build.
It then runs `wasm-bindgen --target web` into `<out_dir>/client` using the
library target name as the output name. `RUSTY_WASM_BINDGEN` overrides the tool
path.

The generated directory is cleared before each run. A sorted
`client-manifest.json` records the format version, package name, JavaScript
entrypoint, and every artifact's relative path, size, and FNV-1a fingerprint.
Client artifact names are not rewritten for cache busting; deployment adapters
can use the manifest fingerprints when publishing. Client compilation runs in
both `rusty build` and `rusty dev`.

`serve()` and `serve_with()` attach `<out_dir>/client` to the router at the
reserved `/_rusty/client` prefix. Requests are decoded and traversal-checked with
the same rules as public assets, and receive the same MIME and cache headers.
Custom routers can call `Router::client_dir`. The prefix is reserved for
generated artifacts and is checked before public files. `CLIENT_URL_PREFIX` is
shared with the bundle metadata so deployment adapters construct matching URLs.

### Async handlers and streaming SSR

`#[rusty::app]` reads each route module's function signatures at compile time.
`async fn` pages are registered with `PageRoute::async_new`; `async fn` API
methods use `ApiHandler::Async`. Sync handlers still use function pointers, so
existing applications do not change. Async handlers return boxed futures, and
the blocking server executes them with `rusty_server::block_on`. The executor
parks the current connection thread on `Pending` and repolls after its waker
fires; it intentionally provides no timer, I/O driver, or thread pool.

`ServerError` propagation and page error boundaries apply after the future
resolves. `rusty-ssr::render_to_chunks` traverses the element tree with an
explicit stack and emits ordered chunks of roughly eight kilobytes without
building one full HTML string. Page responses use `Response::stream_html`, so
the HTTP/1.1 transport emits `transfer-encoding: chunked` instead of
`content-length`. `Response::stream` exposes the same path to middleware and API
handlers. `Response::body()` still returns a complete byte slice by collecting a
stream once; `into_parts()` does the same for transport-neutral callers.

Chunked SSR starts after the page future resolves. Suspending subtrees
mid-document remains future work.

### Loading boundaries and client navigation

`RouteManifest` already records the nearest `loading.rs` for each page.
`#[rusty::app]` registers that handler through `PageRoute::loading`, where the
boundary is a synchronous `fn() -> Element`. During `Router::build`, each
boundary is rendered once into JSON embedded in every page. Route patterns are
turned into anchored regular expressions and ordered by specificity so a
navigated path selects the same boundary a server route would.

Navigation is enabled automatically when at least one page has a boundary.
`Router::navigation(true)` enables it for an application without boundaries, and
`Router::navigation(false)` disables it. The browser runtime intercepts plain
left-click navigation on same-origin links and fetches the destination with an
`X-Rusty-Navigation: 1` header. The target's boundary is rendered in a fixed
status overlay while the request is in flight. Once a successful HTML response
arrives, the runtime calls `history.pushState` and replaces the document with
`document.open`, `document.write`, and `document.close`. Back and forward use the
same fetch-and-replace path.

This deliberately replaces the whole document rather than patching a persistent
application shell. Scripts rerun and client state is recreated, which keeps
hydration correct without introducing teardown or ownership transfer. Links with
`target`, `download`, `rel="external"`, or `data-rusty-reload`, cross-origin and
hash-only links, protected `/_rusty/` paths, forms, and non-HTML responses keep
native behavior. If the destination has no loading boundary, a generic loading
indicator is used. Client-side state preservation, scroll restoration, prefetch,
and form interception remain future work.

### Deployment bundle

`rusty build` copies the native executable, `public` tree, generated client,
manifests, and a rewritten `rusty.toml` into `<out_dir>/dist`. The rewritten
configuration uses `out_dir = "."` and `public_dir = "public"` so the bundle runs
from its own directory with the client mounted at `/_rusty/client`. The bundle
also contains `deployment.json` with the executable name, available artifact
groups, the client URL prefix, and the client entrypoint. When client artifacts
are present, the bundle validates `client-manifest.json`, checks each recorded
size against the generated file, and fails rather than publishing a mismatched
client. The directory is written through a temporary sibling and then swapped
into place, so a failed copy does not leave a partial bundle. Symlinks are
skipped when copying public and client files.

Platform-specific adapters can consume this bundle without rerunning Cargo.
Static hosting, serverless, and container adapters remain future work.

### Development process supervision

`rusty dev` is a process supervisor rather than a separate diagnostics server.
It starts `cargo run --quiet` in the project, sets `RUSTY_DEV=1`, and forwards
`RUSTY_HOST` and `RUSTY_PORT` overrides. A polling snapshot of `.rs`,
`Cargo.toml`, and `rusty.toml` files triggers a rebuild and restart after each
change. `target`, `.git`, and `node_modules` are skipped.

When `build.client_dir` is configured, the supervisor builds the client before
starting the application and rebuilds it only when a client source file, any
`Cargo.toml`, or `rusty.toml` changes. Other application source changes restart
the native process without invoking `wasm-bindgen`. A failed client build keeps
the supervisor running and retries on later changes; the native application is
still started so server-side errors remain visible.

Because procedural macros cannot tell Cargo which files they read, generated
projects include a build script that recursively emits `cargo:rerun-if-changed`
for `src`. This ensures adding or removing a route re-expands the macro.

`rusty build` and `rusty start` delegate to `cargo build` and `cargo run`.
Production WASM client compilation is handled by the CLI's `wasm` module; a
standalone `rusty-build` crate remains a future extraction.

### Manifest format

`AppManifest` embeds the route manifest, the configured source root, the sorted
server-function catalog, and the sorted static asset catalog behind a format
version. `client-manifest.json` separately describes generated WebAssembly client
artifacts through `ClientManifest`, which validates relative unique paths,
entrypoint membership, and fingerprint formatting when written or restored. JSON restoration revalidates route patterns and duplicate shapes, checks
the manifest version, validates asset paths and URLs, and rejects empty names,
modules, or source paths. Build artifacts therefore fail loudly when edited or
produced by an incompatible compiler.

Manifests are written atomically enough for development use: parent directories
are created and the complete JSON string is written in one call. Incremental and
atomic rename behavior belongs to the production build pipeline.

### CLI

The CLI uses a small hand-written parser rather than a dependency. Commands are
explicit and return a typed report for printing by the binary:

- `rusty new <path> [--client]` creates a single-crate application, refuses
  non-empty destinations, derives a sanitized crate name, points the facade
  dependency at the workspace source checkout, and optionally scaffolds a
  WebAssembly client package with a layout bootstrap.
- `rusty check [--project <path>]` analyzes without writing.
- `rusty generate [--project <path>]` analyzes and writes the manifest under the
  configured output directory.
- `rusty build [--project <path>] [--release]` writes the application manifest,
  delegates to Cargo for native compilation, builds the configured WebAssembly
  client, and writes a standalone deployment bundle; `rusty start` delegates to
  `cargo run`.
- `rusty dev [--project <path>] [--host <host>] [--port <port>]` supervises
  `cargo run`, enables development diagnostics, restarts after source changes,
  and rebuilds the configured WebAssembly client when its inputs change.

Usage errors exit with status 2; execution failures exit with status 1.

### Static export example

`examples/site` exports a complete documentation site with `cargo run -p site`.
The generator asks the application router for each known path, writes flat
extensionless targets such as `docs.html` alongside compatibility
`docs/index.html` copies, copies public assets, emits `404.html`, writes a
`sitemap.xml` and `robots.txt`, and adds `.nojekyll`. It supports project-page
base paths through `RUSTY_BASE_PATH`; the generated navigation runtime strips
the same prefix before matching loading boundaries, and links omit trailing
slashes. A dedicated `Deploy site` workflow publishes the artifact through
GitHub Pages and smoke-tests the deployed extensionless routes.

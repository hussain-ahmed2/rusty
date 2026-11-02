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
| `rusty-macros` | `view!`, component and server macros | M1 (`view!`), M2 (`#[component]`) |
| `rusty-ssr` | Server-side HTML rendering | M1 |
| `rusty-reactivity` | Signals, memos, effects, scheduling | M3 |
| `rusty-vdom` | Virtual node snapshots for client rendering | M4 |
| `rusty-dom` | Browser DOM abstraction | M4 |
| `rusty-renderer` | Client mounting, patching, and reactive roots | M4 |
| `rusty-hydrate` | Serialization protocol and hydration | M5 |
| `rusty-router` | File-based routing and route manifests | M6 |
| `rusty-server` | HTTP runtime, middleware, request context | M7 |
| `rusty-rpc` | Typed server functions | M7 |
| `rusty-metadata` | SEO and metadata | M9 |
| `rusty-compiler` | Application analysis and code generation | M8 |
| `rusty-build` | Native/WASM build orchestration | M8 |
| `rusty-cli` | `rusty new`, `dev`, `build`, `start`, `generate` | M8 |
| `rusty-config` | `rusty.toml` parsing and validation | M8 |

## Milestones

**Status:** Milestone 4 complete. Next: Milestone 5 — SSR and hydration.

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

### Milestone 5 — SSR and hydration

SSR integration, deterministic component identities, serialization protocol,
state restoration, mismatch detection, and recovery.

### Milestone 6 — Routing

File discovery for `src/app`, nested layouts, dynamic/catch-all routes, route
groups, loading/error/not-found boundaries, and build-time route manifests.

### Milestone 7 — Full stack

`#[server]` functions, typed RPC, `route.rs` API endpoints, middleware, request
context, authentication hooks, and CSRF protection.

### Milestone 8 — Compiler and tooling

Application compilation, server/client dependency analysis, `rusty.toml`, CLI,
development server, file watching, hot reload, and production builds.

### Milestone 9 — Production features

Static generation, streaming SSR, metadata, caching, image handling, and
deployment support. A documentation website deployed through GitHub Pages will
be added once static generation can produce the site.

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

`rusty-macros` depends on `rusty-core` only for shared validation helpers such as
the void-element table. Generated code does not reference `rusty-core` directly,
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

## Definition of done for each milestone

1. Scope implemented with no placeholders.
2. Public APIs documented.
3. Unit tests cover core behavior and failure cases.
4. Integration/example tests cover the developer-facing path.
5. Formatter, compiler, Clippy, and tests pass.
6. Plan and README status updated.
7. Remaining limitations stated explicitly.

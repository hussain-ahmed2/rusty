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
| `rusty-core` | Element model, names, shared types | M1 |
| `rusty-macros` | `view!`, component and server macros | M1 (`view!`) |
| `rusty-ssr` | Server-side HTML rendering | M1 |
| `rusty-reactivity` | Signals, memos, effects, scheduling | M3 |
| `rusty-vdom` | Initial render representation (not the update engine) | M4 |
| `rusty-dom` | Browser DOM abstraction | M4 |
| `rusty-renderer` | Rendering abstractions | M4 |
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

**Status:** Milestone 1 complete. Next: Milestone 2 — component system.

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

### Milestone 2 — Component system

`#[component]`, typed props, children, fragments, dynamic expressions,
conditionals (`{#if}`), iteration (`{#each}`), and component composition.

### Milestone 3 — Reactivity

Signals, memos, effects, dependency tracking, batching, context, lifecycle, and
cleanup with a cycle-safe ownership model.

### Milestone 4 — Browser runtime

WebAssembly target, DOM abstraction, client rendering, event handling, and
fine-grained DOM updates.

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
deployment support.

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

The tree is intentionally materialized in M1. This keeps SSR simple and makes
the macro independently testable. Reactivity in M3 will add node-level update
handles without retrofitting unsafe shared mutable state.

### `view!` macro

The macro is a hand-written recursive descent parser over `proc_macro2` tokens,
using `syn` for Rust literals, identifiers, spans, and error reporting.

Generated code uses a small hidden helper surface in `rusty::__private` so the
output stays stable and the facade can change internally without breaking
expansions. Tag and attribute names are emitted as `'static` string literals;
the helpers validate them defensively before constructing the tree.

Compile-time diagnostics include:

- unmatched or mismatched closing tags,
- component tags before M2,
- dynamic expressions before M2,
- unsupported node forms.

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

## Definition of done for each milestone

1. Scope implemented with no placeholders.
2. Public APIs documented.
3. Unit tests cover core behavior and failure cases.
4. Integration/example tests cover the developer-facing path.
5. Formatter, compiler, Clippy, and tests pass.
6. Plan and README status updated.
7. Remaining limitations stated explicitly.

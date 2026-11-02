//! Fine-grained, thread-local reactivity for Rusty.
//!
//! The runtime is built around three primitives:
//!
//! - [`Signal`]: owned read-write state.
//! - [`Memo`]: a lazily recomputed derived value.
//! - [`Effect`]: a side effect that reruns when its dependencies change.
//!
//! Signals are `Rc`-backed and are not `Send` or `Sync`. All reactive work runs
//! on the thread that created the value. Batching coalesces notifications, and
//! owners provide deterministic lifecycle cleanup.
//!
//! Create an [`Owner`] with [`create_root`] for deterministic disposal. Effects
//! created outside an explicit root attach to a thread-local root that lives for
//! the rest of the thread.
//!
//! ```
//! use std::cell::Cell;
//! use std::rc::Rc;
//!
//! use rusty_reactivity::{batch, create_effect, create_root, Memo, Signal};
//!
//! let (owner, (count, doubled, runs)) = create_root(|| {
//!     let count = Signal::new(1);
//!     let doubled = Memo::new({
//!         let count = count.clone();
//!         move || count.get() * 2
//!     });
//!     let runs = Rc::new(Cell::new(0));
//!
//!     create_effect({
//!         let doubled = doubled.clone();
//!         let runs = Rc::clone(&runs);
//!         move || {
//!             let _ = doubled.get();
//!             runs.set(runs.get() + 1);
//!         }
//!     });
//!
//!     (count, doubled, runs)
//! });
//!
//! assert_eq!(doubled.get(), 2);
//! count.set(3);
//! assert_eq!(doubled.get(), 6);
//! assert_eq!(runs.get(), 2);
//!
//! batch(|| {
//!     count.set(4);
//!     count.set(5);
//! });
//!
//! assert_eq!(doubled.get(), 10);
//! assert_eq!(runs.get(), 3);
//! owner.dispose();
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod effect;
mod element;
mod memo;
mod owner;
mod runtime;
mod signal;

pub use effect::{create_effect, Effect};
pub use memo::{memo, Memo};
pub use owner::{create_root, expect_context, on_cleanup, provide_context, use_context, Owner};
pub use signal::{signal, Signal};

/// Groups reactive writes so dependent effects are deferred until the batch ends.
///
/// Batches nest. Effects run after the outermost batch completes.
pub fn batch<T>(f: impl FnOnce() -> T) -> T {
    runtime::batch(f)
}

/// Runs `f` without subscribing the current computation to values it reads.
pub fn untrack<T>(f: impl FnOnce() -> T) -> T {
    runtime::untrack(f)
}

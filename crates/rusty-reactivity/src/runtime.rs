//! Thread-local scheduling, dependency tracking, and batching.

use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::rc::{Rc, Weak};

use crate::owner::OwnerInner;

/// A reactive value that subscribers can listen to.
pub(crate) trait Dependency {
    /// Stable identity used to deduplicate subscriptions.
    fn dependency_id(&self) -> usize;

    /// Adds a subscriber. Implementations must ignore duplicates.
    fn subscribe(&self, subscriber: Weak<dyn Subscriber>);

    /// Removes a subscriber by identity.
    fn unsubscribe(&self, subscriber_id: usize);
}

/// A reactive computation that depends on values.
pub(crate) trait Subscriber {
    /// Stable identity used to deduplicate scheduling.
    fn subscriber_id(&self) -> usize;

    /// Records a dependency and returns `true` when it is new.
    fn add_dependency(&self, dependency: Rc<dyn Dependency>) -> bool;

    /// Attaches a cleanup, returning it when the subscriber cannot own it.
    fn add_cleanup(&self, cleanup: Box<dyn FnOnce()>) -> Result<(), Box<dyn FnOnce()>> {
        Err(cleanup)
    }

    /// Marks the subscriber dirty and schedules work when needed.
    fn notify(&self);

    /// Runs a scheduled subscriber. Memos use the default no-op.
    fn run(&self) {}
}

/// A resource owned by an [`Owner`](crate::Owner).
pub(crate) trait Disposable {
    /// Releases the resource. Implementations must be idempotent.
    fn dispose(&self);
}

thread_local! {
    static RUNTIME: Runtime = Runtime::new();
}

struct Runtime {
    owners: RefCell<Vec<Rc<OwnerInner>>>,
    observers: RefCell<Vec<Weak<dyn Subscriber>>>,
    default_owner: Rc<OwnerInner>,
    batch_depth: Cell<usize>,
    computing: Cell<usize>,
    untrack_boundaries: RefCell<Vec<usize>>,
    defer_flush_depth: Cell<usize>,
    running_effects: Cell<usize>,
    next_subscriber_id: Cell<usize>,
    pending: RefCell<VecDeque<(usize, Weak<dyn Subscriber>)>>,
    pending_ids: RefCell<HashSet<usize>>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            owners: RefCell::new(Vec::new()),
            observers: RefCell::new(Vec::new()),
            default_owner: OwnerInner::new_root(),
            batch_depth: Cell::new(0),
            computing: Cell::new(0),
            untrack_boundaries: RefCell::new(Vec::new()),
            defer_flush_depth: Cell::new(0),
            running_effects: Cell::new(0),
            next_subscriber_id: Cell::new(0),
            pending: RefCell::new(VecDeque::new()),
            pending_ids: RefCell::new(HashSet::new()),
        }
    }

    fn flush(&self) {
        if self.batch_depth.get() > 0
            || self.computing.get() > 0
            || !self.untrack_boundaries.borrow().is_empty()
            || self.defer_flush_depth.get() > 0
            || self.running_effects.get() > 0
        {
            return;
        }

        if self.pending.borrow().is_empty() {
            return;
        }

        self.running_effects.set(self.running_effects.get() + 1);
        let _guard = RunningGuard;

        loop {
            let next = {
                let mut pending = self.pending.borrow_mut();

                let Some((id, subscriber)) = pending.pop_front() else {
                    break;
                };

                self.pending_ids.borrow_mut().remove(&id);
                subscriber
            };

            if let Some(subscriber) = next.upgrade() {
                subscriber.run();
            }
        }
    }
}

struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| {
            runtime
                .running_effects
                .set(runtime.running_effects.get().saturating_sub(1));

            if !std::thread::panicking() {
                runtime.flush();
            }
        });
    }
}

/// Returns the current owner, falling back to the thread-local root owner.
pub(crate) fn current_owner() -> Rc<OwnerInner> {
    RUNTIME.with(|runtime| {
        runtime
            .owners
            .borrow()
            .last()
            .cloned()
            .unwrap_or_else(|| Rc::clone(&runtime.default_owner))
    })
}

/// Returns the computation currently collecting dependencies.
pub(crate) fn current_subscriber() -> Option<Rc<dyn Subscriber>> {
    RUNTIME.with(|runtime| runtime.observers.borrow().last().and_then(Weak::upgrade))
}

/// Records a dependency on the current computation, if any.
pub(crate) fn track(dependency: Rc<dyn Dependency>) {
    RUNTIME.with(|runtime| {
        let observer_depth = runtime.observers.borrow().len();

        if runtime
            .untrack_boundaries
            .borrow()
            .iter()
            .any(|boundary| observer_depth <= *boundary)
        {
            return;
        }

        let Some(observer) = runtime.observers.borrow().last().cloned() else {
            return;
        };
        let Some(subscriber) = observer.upgrade() else {
            return;
        };

        if subscriber.add_dependency(dependency.clone()) {
            dependency.subscribe(observer);
        }
    });
}

/// Allocates a stable identity for a reactive subscriber.
pub(crate) fn next_subscriber_id() -> usize {
    RUNTIME.with(|runtime| {
        let id = runtime.next_subscriber_id.get();
        runtime.next_subscriber_id.set(id.wrapping_add(1));
        id
    })
}

/// Removes a disposed subscriber's queued work.
pub(crate) fn cancel(subscriber_id: usize) {
    RUNTIME.with(|runtime| {
        runtime.pending_ids.borrow_mut().remove(&subscriber_id);
        runtime
            .pending
            .borrow_mut()
            .retain(|(id, _)| *id != subscriber_id);
    });
}

/// Schedules a subscriber to run after the current reactive work settles.
pub(crate) fn enqueue(subscriber: Weak<dyn Subscriber>) {
    RUNTIME.with(|runtime| {
        let Some(id) = subscriber
            .upgrade()
            .map(|subscriber| subscriber.subscriber_id())
        else {
            return;
        };

        if runtime.pending_ids.borrow_mut().insert(id) {
            runtime.pending.borrow_mut().push_back((id, subscriber));
        }
    });
}

/// Runs pending effects when no batch or computation is active.
pub(crate) fn flush_if_ready() {
    RUNTIME.with(Runtime::flush);
}

/// Registers an effect or other disposable with the current owner.
pub(crate) fn register_disposable(disposable: Rc<dyn Disposable>) {
    current_owner().add_disposable(disposable);
}

/// Registers an owner-owned cleanup.
pub(crate) fn register_owner_cleanup(cleanup: Box<dyn FnOnce()>) {
    current_owner().add_cleanup(cleanup);
}

/// Defers effect flushing while a borrowed value is being used.
pub(crate) struct DeferFlushGuard;

impl DeferFlushGuard {
    pub(crate) fn enter() -> Self {
        RUNTIME.with(|runtime| {
            runtime
                .defer_flush_depth
                .set(runtime.defer_flush_depth.get() + 1);
        });
        Self
    }
}

impl Drop for DeferFlushGuard {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| {
            runtime
                .defer_flush_depth
                .set(runtime.defer_flush_depth.get().saturating_sub(1));

            if runtime.defer_flush_depth.get() == 0 && !std::thread::panicking() {
                runtime.flush();
            }
        });
    }
}

/// Runs `f` while suppressing dependency tracking.
pub(crate) fn untrack<T>(f: impl FnOnce() -> T) -> T {
    RUNTIME.with(|runtime| {
        let boundary = runtime.observers.borrow().len();
        runtime.untrack_boundaries.borrow_mut().push(boundary);
    });

    let guard = UntrackGuard;
    let value = f();
    drop(guard);
    value
}

struct UntrackGuard;

impl Drop for UntrackGuard {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| {
            runtime.untrack_boundaries.borrow_mut().pop();

            if runtime.untrack_boundaries.borrow().is_empty() && !std::thread::panicking() {
                runtime.flush();
            }
        });
    }
}

/// Runs `f`, deferring effect execution until the outermost batch completes.
pub(crate) fn batch<T>(f: impl FnOnce() -> T) -> T {
    RUNTIME.with(|runtime| {
        runtime.batch_depth.set(runtime.batch_depth.get() + 1);
    });

    let guard = BatchGuard;
    let value = f();
    drop(guard);
    value
}

struct BatchGuard;

impl Drop for BatchGuard {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| {
            runtime
                .batch_depth
                .set(runtime.batch_depth.get().saturating_sub(1));

            if runtime.batch_depth.get() == 0 && !std::thread::panicking() {
                runtime.flush();
            }
        });
    }
}

/// Pushes an owner while a root is active.
pub(crate) struct OwnerGuard;

impl OwnerGuard {
    pub(crate) fn enter(owner: Rc<OwnerInner>) -> Self {
        RUNTIME.with(|runtime| runtime.owners.borrow_mut().push(owner));
        Self
    }
}

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| {
            runtime.owners.borrow_mut().pop();
        });
    }
}

/// Pushes a computation that collects dependencies.
pub(crate) struct ObserverGuard;

impl ObserverGuard {
    pub(crate) fn enter(observer: Weak<dyn Subscriber>) -> Self {
        RUNTIME.with(|runtime| {
            runtime.observers.borrow_mut().push(observer);
            runtime.computing.set(runtime.computing.get() + 1);
        });
        Self
    }
}

impl Drop for ObserverGuard {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| {
            runtime.observers.borrow_mut().pop();
            runtime
                .computing
                .set(runtime.computing.get().saturating_sub(1));
        });
    }
}

/// Notifies every live subscriber in a subscription list.
pub(crate) fn notify_subscribers(subscribers: &RefCell<Vec<Weak<dyn Subscriber>>>) {
    let snapshot = subscribers.borrow().clone();

    for subscriber in &snapshot {
        if let Some(subscriber) = subscriber.upgrade() {
            subscriber.notify();
        }
    }

    subscribers
        .borrow_mut()
        .retain(|subscriber| subscriber.upgrade().is_some());
}

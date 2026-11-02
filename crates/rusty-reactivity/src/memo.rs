//! Lazily recomputed derived values.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::{Rc, Weak};

use crate::runtime::{self, Dependency, Subscriber};

/// A lazily computed value derived from other reactive values.
///
/// A memo recomputes only when read after one of its dependencies changes. Its
/// subscribers are notified when it becomes dirty; the value itself is computed
/// on the next read.
///
/// Do not capture a memo's own strongly held handle inside its computation. Use
/// a `Weak` if a computation genuinely needs to refer to the memo.
pub struct Memo<T: 'static> {
    inner: Rc<MemoInner<T>>,
}

struct MemoInner<T: 'static> {
    id: usize,
    compute: RefCell<Box<dyn FnMut() -> T>>,
    value: RefCell<Option<T>>,
    dirty: Cell<bool>,
    computing: Cell<bool>,
    dependencies: RefCell<Vec<Rc<dyn Dependency>>>,
    subscribers: RefCell<Vec<Weak<dyn Subscriber>>>,
    self_weak: OnceCell<Weak<dyn Subscriber>>,
}

impl<T: 'static> Memo<T> {
    /// Creates a memo from a computation.
    ///
    /// The computation is not run until the first read.
    #[must_use]
    pub fn new<F>(compute: F) -> Self
    where
        F: FnMut() -> T + 'static,
    {
        let inner = Rc::new(MemoInner {
            id: runtime::next_subscriber_id(),
            compute: RefCell::new(Box::new(compute)),
            value: RefCell::new(None),
            dirty: Cell::new(true),
            computing: Cell::new(false),
            dependencies: RefCell::new(Vec::new()),
            subscribers: RefCell::new(Vec::new()),
            self_weak: OnceCell::new(),
        });

        let strong: Rc<dyn Subscriber> = inner.clone();
        let weak = Rc::downgrade(&strong);
        let _ = inner.self_weak.set(weak);

        Self { inner }
    }

    /// Returns a clone of the current value, computing it when dirty.
    ///
    /// Reading a memo inside an effect or memo subscribes the reader to it.
    #[must_use]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.ensure_value();
        self.track();
        self.inner
            .value
            .borrow()
            .as_ref()
            .expect("memo value must exist after computation")
            .clone()
    }

    /// Returns a clone of the current value without tracking the read.
    #[must_use]
    pub fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        self.ensure_value();
        self.inner
            .value
            .borrow()
            .as_ref()
            .expect("memo value must exist after computation")
            .clone()
    }

    /// Borrows the current value for `f`, computing it when dirty.
    ///
    /// Writes performed inside `f` are deferred until the borrow ends.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.ensure_value();
        self.track();
        let _defer = runtime::DeferFlushGuard::enter();
        let value = self.inner.value.borrow();
        f(value
            .as_ref()
            .expect("memo value must exist after computation"))
    }

    fn ensure_value(&self) {
        if self.inner.computing.get() {
            panic!("memo re-entered while computing; check for a dependency cycle");
        }

        if self.inner.dirty.get() || self.inner.value.borrow().is_none() {
            self.inner.recompute();
        }
    }

    fn track(&self) {
        let dependency: Rc<dyn Dependency> = self.inner.clone();
        runtime::track(dependency);
    }
}

impl<T: 'static> MemoInner<T> {
    fn recompute(&self) {
        if self.computing.replace(true) {
            panic!("memo re-entered while computing; check for a dependency cycle");
        }

        let mut guard = ComputeGuard {
            computing: &self.computing,
            dirty: &self.dirty,
            succeeded: false,
        };

        self.dirty.set(false);

        let subscriber_id = self.subscriber_id();
        let dependencies = std::mem::take(&mut *self.dependencies.borrow_mut());

        for dependency in dependencies {
            dependency.unsubscribe(subscriber_id);
        }

        let observer = runtime::ObserverGuard::enter(
            self.self_weak
                .get()
                .expect("memo subscriber weak must be initialized")
                .clone(),
        );
        let value = {
            let mut compute = self.compute.borrow_mut();
            compute()
        };
        drop(observer);

        *self.value.borrow_mut() = Some(value);
        guard.succeeded = true;
        drop(guard);

        runtime::flush_if_ready();
    }

    fn notify_subscribers(&self) {
        runtime::notify_subscribers(&self.subscribers);
    }
}

impl<T: 'static> Dependency for MemoInner<T> {
    fn dependency_id(&self) -> usize {
        self as *const Self as usize
    }

    fn subscribe(&self, subscriber: Weak<dyn Subscriber>) {
        let Some(subscriber_id) = subscriber.upgrade().map(|value| value.subscriber_id()) else {
            return;
        };

        let mut subscribers = self.subscribers.borrow_mut();
        subscribers.retain(|existing| existing.upgrade().is_some());

        if subscribers.iter().any(|existing| {
            existing
                .upgrade()
                .is_some_and(|value| value.subscriber_id() == subscriber_id)
        }) {
            return;
        }

        subscribers.push(subscriber);
    }

    fn unsubscribe(&self, subscriber_id: usize) {
        self.subscribers.borrow_mut().retain(|subscriber| {
            subscriber
                .upgrade()
                .is_some_and(|value| value.subscriber_id() != subscriber_id)
        });
    }
}

impl<T: 'static> Subscriber for MemoInner<T> {
    fn subscriber_id(&self) -> usize {
        self.id
    }

    fn add_dependency(&self, dependency: Rc<dyn Dependency>) -> bool {
        let dependency_id = dependency.dependency_id();
        let mut dependencies = self.dependencies.borrow_mut();

        if dependencies
            .iter()
            .any(|existing| existing.dependency_id() == dependency_id)
        {
            return false;
        }

        dependencies.push(dependency);
        true
    }

    fn notify(&self) {
        if self.dirty.replace(true) {
            return;
        }

        self.notify_subscribers();
    }
}

impl<T: 'static> Drop for MemoInner<T> {
    fn drop(&mut self) {
        let subscriber_id = self.subscriber_id();

        for dependency in self.dependencies.get_mut().drain(..) {
            dependency.unsubscribe(subscriber_id);
        }
    }
}

impl<T: 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

struct ComputeGuard<'a> {
    computing: &'a Cell<bool>,
    dirty: &'a Cell<bool>,
    succeeded: bool,
}

impl Drop for ComputeGuard<'_> {
    fn drop(&mut self) {
        self.computing.set(false);

        if !self.succeeded {
            self.dirty.set(true);
        }
    }
}

/// Creates a lazily computed value from a function.
#[must_use]
pub fn memo<T: 'static, F>(compute: F) -> Memo<T>
where
    F: FnMut() -> T + 'static,
{
    Memo::new(compute)
}

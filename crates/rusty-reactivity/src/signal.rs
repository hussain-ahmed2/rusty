//! Read-write reactive values.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::runtime::{self, Dependency, Subscriber};

/// A reactive, single-threaded value.
///
/// Reading a signal inside an effect or memo subscribes that computation to the
/// signal. Writing a signal notifies its subscribers; effect runs are deferred
/// until the current batch completes.
///
/// Signals are `Rc`-backed and are not `Send` or `Sync`. Create and use them on
/// the UI thread.
pub struct Signal<T: 'static> {
    inner: Rc<SignalInner<T>>,
}

pub(crate) struct SignalInner<T: 'static> {
    value: RefCell<T>,
    subscribers: RefCell<Vec<Weak<dyn Subscriber>>>,
}

impl<T: 'static> Signal<T> {
    /// Creates a signal with an initial value.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            inner: Rc::new(SignalInner {
                value: RefCell::new(value),
                subscribers: RefCell::new(Vec::new()),
            }),
        }
    }

    /// Returns a clone of the current value and tracks the read.
    #[must_use]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.track();
        self.inner.value.borrow().clone()
    }

    /// Returns a clone of the current value without tracking the read.
    #[must_use]
    pub fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        self.inner.value.borrow().clone()
    }

    /// Borrows the current value for the duration of `f` and tracks the read.
    ///
    /// Do not write to the same signal inside `f`; the value is already borrowed.
    /// Writes to other signals are deferred until the borrow ends.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        let _defer = runtime::DeferFlushGuard::enter();
        let value = self.inner.value.borrow();
        f(&value)
    }

    /// Borrows the current value for `f` without tracking the read.
    ///
    /// Do not write to the same signal inside `f`; the value is already borrowed.
    /// Writes to other signals are deferred until the borrow ends.
    pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let _defer = runtime::DeferFlushGuard::enter();
        let value = self.inner.value.borrow();
        f(&value)
    }

    /// Replaces the value and notifies subscribers.
    pub fn set(&self, value: T) {
        *self.inner.value.borrow_mut() = value;
        self.inner.notify();
    }

    /// Replaces the value only when it differs from the current value.
    ///
    /// Returns `true` when a write occurred.
    pub fn set_if_changed(&self, value: T) -> bool
    where
        T: PartialEq,
    {
        let changed = {
            let current = self.inner.value.borrow();
            *current != value
        };

        if changed {
            self.set(value);
        }

        changed
    }

    /// Mutates the value in place and notifies subscribers.
    pub fn update(&self, update: impl FnOnce(&mut T)) {
        {
            let mut value = self.inner.value.borrow_mut();
            update(&mut value);
        }

        self.inner.notify();
    }

    fn track(&self) {
        let dependency: Rc<dyn Dependency> = self.inner.clone();
        runtime::track(dependency);
    }
}

impl<T: 'static> SignalInner<T> {
    fn notify(&self) {
        runtime::notify_subscribers(&self.subscribers);
        runtime::flush_if_ready();
    }
}

impl<T: 'static> Dependency for SignalInner<T> {
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

impl<T: 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: 'static> From<T> for Signal<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

/// Creates a signal with an initial value.
#[must_use]
pub fn signal<T: 'static>(value: T) -> Signal<T> {
    Signal::new(value)
}

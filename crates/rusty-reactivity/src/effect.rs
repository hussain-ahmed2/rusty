//! Side effects and lifecycle cleanup.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::{Rc, Weak};

use crate::runtime::{self, Dependency, Disposable, Subscriber};

/// A reactive side effect.
///
/// The effect runs immediately, tracks every signal it reads, and reruns after a
/// dependency changes. Effects are owned by the current [`Owner`](crate::Owner)
/// and are disposed with it. Keep the handle to dispose early.
///
/// Avoid capturing the effect's own strongly held handle inside its callback.
/// Also avoid unconditionally writing a signal that the effect reads; that
/// intentionally schedules another run and can loop forever.
pub struct Effect {
    inner: Rc<EffectInner>,
}

struct EffectInner {
    id: usize,
    callback: RefCell<Option<Box<dyn FnMut()>>>,
    dependencies: RefCell<Vec<Rc<dyn Dependency>>>,
    cleanups: RefCell<Vec<Box<dyn FnOnce()>>>,
    self_weak: OnceCell<Weak<dyn Subscriber>>,
    disposed: Cell<bool>,
    running: Cell<bool>,
}

impl Effect {
    /// Creates and immediately runs an effect.
    #[must_use]
    pub fn new<F>(callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        let inner = Rc::new(EffectInner {
            id: runtime::next_subscriber_id(),
            callback: RefCell::new(Some(Box::new(callback))),
            dependencies: RefCell::new(Vec::new()),
            cleanups: RefCell::new(Vec::new()),
            self_weak: OnceCell::new(),
            disposed: Cell::new(false),
            running: Cell::new(false),
        });

        let strong: Rc<dyn Subscriber> = inner.clone();
        let weak = Rc::downgrade(&strong);
        let _ = inner.self_weak.set(weak);

        let disposable: Rc<dyn Disposable> = inner.clone();
        runtime::register_disposable(disposable);

        let effect = Self { inner };
        effect.inner.run();
        runtime::flush_if_ready();
        effect
    }

    /// Returns `true` when the effect has been disposed.
    #[must_use]
    pub fn is_disposed(&self) -> bool {
        self.inner.disposed.get()
    }

    /// Disposes the effect, unsubscribing it and running its cleanups.
    pub fn dispose(&self) {
        self.inner.dispose();
    }
}

impl EffectInner {
    fn run(&self) {
        if self.disposed.get() || self.running.replace(true) {
            return;
        }

        let running = RunningGuard(&self.running);
        self.clear_dependencies();
        self.run_cleanups();

        let Some(observer) = self.self_weak.get().cloned() else {
            return;
        };

        let observer = runtime::ObserverGuard::enter(observer);

        {
            let mut callback = self.callback.borrow_mut();

            if let Some(callback) = callback.as_mut() {
                callback();
            }
        }

        drop(observer);
        drop(running);

        if self.disposed.get() {
            self.callback.borrow_mut().take();
        }
    }

    fn clear_dependencies(&self) {
        let subscriber_id = self.subscriber_id();
        let dependencies = std::mem::take(&mut *self.dependencies.borrow_mut());

        for dependency in dependencies {
            dependency.unsubscribe(subscriber_id);
        }
    }

    fn run_cleanups(&self) {
        let cleanups = std::mem::take(&mut *self.cleanups.borrow_mut());

        if cleanups.is_empty() {
            return;
        }

        runtime::untrack(|| {
            for cleanup in cleanups {
                cleanup();
            }
        });
    }

    fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }

        runtime::cancel(self.id);
        self.clear_dependencies();
        self.run_cleanups();

        if !self.running.get() {
            self.callback.borrow_mut().take();
        }
    }
}

impl Subscriber for EffectInner {
    fn subscriber_id(&self) -> usize {
        self.id
    }

    fn add_dependency(&self, dependency: Rc<dyn Dependency>) -> bool {
        if self.disposed.get() {
            return false;
        }

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

    fn add_cleanup(&self, cleanup: Box<dyn FnOnce()>) -> Result<(), Box<dyn FnOnce()>> {
        if self.disposed.get() {
            cleanup();
        } else {
            self.cleanups.borrow_mut().push(cleanup);
        }

        Ok(())
    }

    fn notify(&self) {
        if self.disposed.get() {
            return;
        }

        if let Some(observer) = self.self_weak.get() {
            runtime::enqueue(observer.clone());
        }
    }

    fn run(&self) {
        Self::run(self);
    }
}

impl Disposable for EffectInner {
    fn dispose(&self) {
        Self::dispose(self);
    }
}

struct RunningGuard<'a>(&'a Cell<bool>);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Creates an effect that runs immediately and reruns when dependencies change.
#[must_use]
pub fn create_effect<F>(callback: F) -> Effect
where
    F: FnMut() + 'static,
{
    Effect::new(callback)
}

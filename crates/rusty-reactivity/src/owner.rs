//! Ownership, lifecycle cleanup, and context lookup.

use std::any::{type_name, Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::runtime::{self, Disposable};

/// A lifecycle scope that owns effects, cleanups, and context values.
///
/// Dropping or calling [`Owner::dispose`] releases everything created while the
/// owner was current. Owners form a parent-child tree; parent links are weak so
/// the tree cannot leak through reference cycles.
pub struct Owner {
    inner: Rc<OwnerInner>,
}

impl Owner {
    /// Returns `true` after the owner and its children have been disposed.
    #[must_use]
    pub fn is_disposed(&self) -> bool {
        self.inner.disposed.get()
    }

    /// Disposes the owner immediately.
    pub fn dispose(self) {
        self.inner.dispose();
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.inner.dispose();
    }
}

pub(crate) struct OwnerInner {
    parent: RefCell<Option<Weak<OwnerInner>>>,
    children: RefCell<Vec<Rc<OwnerInner>>>,
    disposables: RefCell<Vec<Rc<dyn Disposable>>>,
    cleanups: RefCell<Vec<Box<dyn FnOnce()>>>,
    contexts: RefCell<HashMap<TypeId, Rc<dyn Any>>>,
    disposed: Cell<bool>,
}

impl OwnerInner {
    pub(crate) fn new_root() -> Rc<Self> {
        Rc::new(Self::new_inner(None))
    }

    pub(crate) fn new_child(parent: Rc<OwnerInner>) -> Rc<Self> {
        let inner = Rc::new(Self::new_inner(Some(Rc::downgrade(&parent))));

        if !parent.disposed.get() {
            parent.children.borrow_mut().push(Rc::clone(&inner));
        }

        inner
    }

    fn new_inner(parent: Option<Weak<OwnerInner>>) -> Self {
        Self {
            parent: RefCell::new(parent),
            children: RefCell::new(Vec::new()),
            disposables: RefCell::new(Vec::new()),
            cleanups: RefCell::new(Vec::new()),
            contexts: RefCell::new(HashMap::new()),
            disposed: Cell::new(false),
        }
    }

    pub(crate) fn add_disposable(&self, disposable: Rc<dyn Disposable>) {
        if self.disposed.get() {
            disposable.dispose();
        } else {
            self.disposables.borrow_mut().push(disposable);
        }
    }

    pub(crate) fn add_cleanup(&self, cleanup: Box<dyn FnOnce()>) {
        if self.disposed.get() {
            cleanup();
        } else {
            self.cleanups.borrow_mut().push(cleanup);
        }
    }

    pub(crate) fn provide(&self, id: TypeId, value: Rc<dyn Any>) {
        if !self.disposed.get() {
            self.contexts.borrow_mut().insert(id, value);
        }
    }

    pub(crate) fn context<T: 'static>(&self) -> Option<Rc<T>> {
        if let Some(value) = self.contexts.borrow().get(&TypeId::of::<T>()).cloned() {
            return value.downcast::<T>().ok();
        }

        let parent = self.parent.borrow().as_ref().and_then(Weak::upgrade);
        parent.and_then(|parent| parent.context::<T>())
    }

    pub(crate) fn dispose(self: &Rc<Self>) {
        if self.disposed.replace(true) {
            return;
        }

        runtime::batch(|| {
            let _owner = runtime::OwnerGuard::enter(Rc::clone(self));
            let parent = self.parent.borrow().as_ref().and_then(Weak::upgrade);

            if let Some(parent) = parent {
                parent
                    .children
                    .borrow_mut()
                    .retain(|child| !Rc::ptr_eq(child, self));
            }

            let children = std::mem::take(&mut *self.children.borrow_mut());

            for child in children {
                child.dispose();
            }

            let disposables = std::mem::take(&mut *self.disposables.borrow_mut());

            for disposable in disposables {
                disposable.dispose();
            }

            let cleanups = std::mem::take(&mut *self.cleanups.borrow_mut());

            for cleanup in cleanups {
                cleanup();
            }

            self.contexts.borrow_mut().clear();
        });
    }
}

/// Creates a lifecycle scope and runs `f` while it is current.
///
/// The returned owner must be held for the scope to stay alive. Dropping or
/// disposing it releases effects, cleanups, child owners, and context values.
pub fn create_root<F, T>(f: F) -> (Owner, T)
where
    F: FnOnce() -> T,
{
    let parent = runtime::current_owner();
    let inner = OwnerInner::new_child(parent);
    let owner = Owner {
        inner: Rc::clone(&inner),
    };
    let guard = runtime::OwnerGuard::enter(inner);
    let value = f();
    drop(guard);

    (owner, value)
}

/// Registers a cleanup on the current effect, or the current owner outside one.
///
/// Cleanups attached to an effect run before the effect reruns and when it is
/// disposed. Owner cleanups run when the owner is disposed.
pub fn on_cleanup<F>(cleanup: F)
where
    F: FnOnce() + 'static,
{
    let cleanup: Box<dyn FnOnce()> = Box::new(cleanup);

    if let Some(subscriber) = runtime::current_subscriber() {
        match subscriber.add_cleanup(cleanup) {
            Ok(()) => return,
            Err(cleanup) => {
                runtime::register_owner_cleanup(cleanup);
                return;
            }
        }
    }

    runtime::register_owner_cleanup(cleanup);
}

/// Provides a typed context value to the current owner and its descendants.
///
/// Returns a shared handle to the stored value.
pub fn provide_context<T: 'static>(value: T) -> Rc<T> {
    let value = Rc::new(value);
    let erased: Rc<dyn Any> = Rc::clone(&value) as Rc<dyn Any>;
    runtime::current_owner().provide(TypeId::of::<T>(), erased);
    value
}

/// Looks up the nearest context value of type `T`.
#[must_use]
pub fn use_context<T: 'static>() -> Option<Rc<T>> {
    runtime::current_owner().context::<T>()
}

/// Looks up a context value or panics with a descriptive message.
///
/// # Panics
///
/// Panics when no owner in the current tree provides a `T`.
#[must_use]
pub fn expect_context<T: 'static>() -> Rc<T> {
    use_context::<T>().unwrap_or_else(|| {
        panic!(
            "no context of type `{}` found; provide it in an ancestor owner",
            type_name::<T>()
        )
    })
}

//! A minimal blocking executor for request-scoped futures.

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

struct ThreadWake(Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Runs a future to completion on the current thread.
///
/// The blocking server already runs each connection on its own thread, so this
/// executor deliberately does not own a thread pool, timers, or I/O drivers. A
/// `Pending` future parks the current thread until its waker fires.
///
/// # Examples
///
/// ```
/// assert_eq!(rusty_server::block_on(async { 20 + 22 }), 42);
/// ```
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);

    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll};

    use super::block_on;

    #[test]
    fn runs_ready_futures() {
        assert_eq!(block_on(async { 20 + 22 }), 42);
    }

    struct WakeOnce {
        woken: Arc<AtomicBool>,
    }

    impl Future for WakeOnce {
        type Output = u32;

        fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
            if self.woken.swap(false, Ordering::AcqRel) {
                Poll::Ready(7)
            } else {
                self.woken.store(true, Ordering::Release);
                context.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    #[test]
    fn parks_and_repolls_after_a_wake() {
        let woken = Arc::new(AtomicBool::new(false));
        assert_eq!(block_on(WakeOnce { woken }), 7);
    }
}

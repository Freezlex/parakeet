use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};

#[derive(Debug, Clone, PartialEq, Default)]
pub enum AsyncState<T, E> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(E),
}

impl<T, E> AsyncState<T, E> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            AsyncState::Ready(v) => Some(v),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, AsyncState::Loading)
    }

    pub fn from_result(r: Result<T, E>) -> Self {
        match r {
            Ok(v) => AsyncState::Ready(v),
            Err(e) => AsyncState::Failed(e),
        }
    }
}

pub fn block_on<F: Future>(mut fut: F) -> F::Output {
    struct Signal {
        wakeup: Mutex<bool>,
        cv: Condvar,
    }

    impl Wake for Signal {
        fn wake(self: Arc<Self>) {
            *self.wakeup.lock().unwrap() = true;
            self.cv.notify_one();
        }
    }

    let signal = Arc::new(Signal { wakeup: Mutex::new(false), cv: Condvar::new() });
    let waker = Waker::from(signal.clone());
    let mut cx = Context::from_waker(&waker);

    let mut fut = unsafe { Pin::new_unchecked(&mut fut) };

    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => {
                let mut wakeup = signal.wakeup.lock().unwrap();
                while !*wakeup {
                    wakeup = signal.cv.wait(wakeup).unwrap();
                }
                *wakeup = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_on_simple_future_resolve() {
        assert_eq!(block_on(async { 1 + 1 }), 2);
    }

    #[test]
    fn lock_on_multiple_future_resolve() {
        async fn deux() -> u32 {
            2
        }
        let r = block_on(async { deux().await + deux().await });
        assert_eq!(r, 4);
    }

    #[test]
    fn async_state_disallow_absurd_states() {
        let s: AsyncState<u8, String> = AsyncState::Loading;
        assert!(s.is_loading());
        assert!(s.ready().is_none());

        let s = AsyncState::<u8, String>::from_result(Ok(7));
        assert_eq!(s.ready(), Some(&7));
        assert!(!s.is_loading());
    }
}

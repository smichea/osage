// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A few threads kept from one block to the next, to share a block's work
//! among: starting threads each block cost 235 µs for three, a tenth of a
//! block; waking these costs some 25 µs.
//!
//! [`Pool::run`] calls a task once for each thread, the first on the caller's
//! own, and returns when all have returned — as `std::thread::scope` does,
//! so the task may borrow what the caller holds. A task that panics on
//! another thread panics the caller.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

/// A task as the threads see it: what it borrows is held by a caller that
/// waits for every thread to be done with it ([`Pool::run`]).
#[derive(Clone, Copy)]
struct Task(*const (dyn Fn(usize) + Sync + 'static));

// SAFETY: the task is `Sync`, so calling it from other threads is sound;
// the pointer is only used while `Pool::run` waits.
unsafe impl Send for Task {}

struct State {
    /// Counts the tasks given: a thread works when it has not seen this one.
    given: u64,
    task: Option<Task>,
    /// Threads still at it.
    pending: usize,
    panicked: bool,
    quit: bool,
}

struct Shared {
    state: Mutex<State>,
    start: Condvar,
    done: Condvar,
}

pub struct Pool {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl Pool {
    /// A pool of `threads`, the caller's own among them: `threads − 1` are
    /// started.
    pub fn new(threads: usize) -> Pool {
        let shared = Arc::new(Shared {
            state: Mutex::new(State { given: 0, task: None, pending: 0, panicked: false, quit: false }),
            start: Condvar::new(),
            done: Condvar::new(),
        });
        let workers = (1..threads.max(1))
            .map(|index| {
                let shared = shared.clone();
                std::thread::Builder::new()
                    .name(format!("osage-sound-{index}"))
                    .spawn(move || work(&shared, index))
                    .expect("a thread for the sound")
            })
            .collect();
        Pool { shared, workers }
    }

    /// How many threads share a task, the caller's own among them.
    pub fn threads(&self) -> usize {
        self.workers.len() + 1
    }

    /// Calls `task(index)` once for each index below [`Pool::threads`], 0 on
    /// this thread, and returns when every call has returned.
    pub fn run(&self, task: &(dyn Fn(usize) + Sync)) {
        if self.workers.is_empty() {
            task(0);
            return;
        }
        // SAFETY: the reference is made to look as if it lived for ever so
        // that the threads may hold it; this function does not return — nor
        // unwind — before every thread has dropped it (`pending` is back at
        // 0 and the task taken back), so it is never used past its life.
        let erased: &(dyn Fn(usize) + Sync + 'static) = unsafe { std::mem::transmute(task) };
        {
            let mut state = self.shared.state.lock().expect("the pool's lock");
            state.task = Some(Task(erased));
            state.pending = self.workers.len();
            state.given += 1;
            self.shared.start.notify_all();
        }
        let mine = catch_unwind(AssertUnwindSafe(|| task(0)));
        let panicked = {
            let mut state = self.shared.state.lock().expect("the pool's lock");
            while state.pending > 0 {
                state = self.shared.done.wait(state).expect("the pool's lock");
            }
            state.task = None;
            std::mem::take(&mut state.panicked)
        };
        if let Err(why) = mine {
            std::panic::resume_unwind(why);
        }
        assert!(!panicked, "a thread of the sound panicked");
    }
}

fn work(shared: &Shared, index: usize) {
    let mut seen = 0;
    loop {
        let task = {
            let mut state = shared.state.lock().expect("the pool's lock");
            while state.given == seen && !state.quit {
                state = shared.start.wait(state).expect("the pool's lock");
            }
            if state.quit {
                return;
            }
            seen = state.given;
            state.task.expect("a task given")
        };
        // SAFETY: `Pool::run` holds what the task borrows until `pending`
        // falls to 0, which this thread does only after its call.
        let call = unsafe { &*task.0 };
        let failed = catch_unwind(AssertUnwindSafe(|| call(index))).is_err();
        let mut state = shared.state.lock().expect("the pool's lock");
        state.panicked |= failed;
        state.pending -= 1;
        if state.pending == 0 {
            shared.done.notify_one();
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.quit = true;
        }
        self.shared.start.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn every_thread_does_its_part_and_the_caller_waits_for_all() {
        let pool = Pool::new(4);
        assert_eq!(pool.threads(), 4);
        let mut parts = [0usize; 4];
        for round in 1..=500 {
            let slots: Vec<Mutex<&mut usize>> = parts.iter_mut().map(Mutex::new).collect();
            pool.run(&|index| {
                **slots[index].lock().unwrap() += round;
            });
        }
        assert!(parts.iter().all(|&part| part == 500 * 501 / 2), "{parts:?}");
        let one = Pool::new(1);
        let calls = AtomicUsize::new(0);
        one.run(&|index| {
            assert_eq!(index, 0);
            calls.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(calls.into_inner(), 1);
    }

    #[test]
    fn a_task_that_panics_on_another_thread_panics_the_caller_and_the_pool_goes_on() {
        let pool = Pool::new(3);
        let failed = catch_unwind(AssertUnwindSafe(|| {
            pool.run(&|index| {
                if index == 2 {
                    panic!("on purpose");
                }
            })
        }));
        assert!(failed.is_err());
        let calls = AtomicUsize::new(0);
        pool.run(&|_| {
            calls.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(calls.into_inner(), 3);
    }
}

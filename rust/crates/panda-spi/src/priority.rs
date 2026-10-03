use std::sync::{Condvar, Mutex};

#[derive(Default)]
struct State {
    active: bool,
    waiters: [u32; 3],
}
#[derive(Default)]
pub(crate) struct Arbiter {
    state: Mutex<State>,
    changed: Condvar,
}
pub(crate) struct Guard<'a>(&'a Arbiter);
impl Arbiter {
    #[cfg(test)]
    pub(crate) fn waiters(&self) -> [u32; 3] {
        self.state.lock().unwrap().waiters
    }
    pub(crate) fn acquire(&self, endpoint: u8) -> Guard<'_> {
        let priority = match endpoint {
            0x03 => 2,
            0x81 => 1,
            _ => 0,
        };
        let mut state = self.state.lock().expect("SPI priority mutex poisoned");
        state.waiters[priority] += 1;
        while state.active || state.waiters[priority + 1..].iter().any(|n| *n > 0) {
            state = self
                .changed
                .wait(state)
                .expect("SPI priority mutex poisoned");
        }
        state.waiters[priority] -= 1;
        state.active = true;
        Guard(self)
    }
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0
            .state
            .lock()
            .expect("SPI priority mutex poisoned")
            .active = false;
        self.0.changed.notify_all();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn queued_can_transmit_precedes_receive_and_control() {
        let arbiter = Arc::new(Arbiter::default());
        let held = arbiter.acquire(0);
        let order = Arc::new(Mutex::new(Vec::new()));
        let workers: Vec<_> = [0, 0x81, 3]
            .into_iter()
            .map(|endpoint| {
                let arbiter = Arc::clone(&arbiter);
                let order = Arc::clone(&order);
                std::thread::spawn(move || {
                    let _guard = arbiter.acquire(endpoint);
                    order.lock().unwrap().push(endpoint);
                })
            })
            .collect();
        while arbiter.state.lock().unwrap().waiters != [1, 1, 1] {
            std::thread::yield_now();
        }
        drop(held);
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), [3, 0x81, 0]);
    }
}

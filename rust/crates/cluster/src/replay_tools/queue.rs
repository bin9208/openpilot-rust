use std::sync::Mutex;

pub struct Latest<T> {
    value: Mutex<Option<T>>,
}
impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }
}
impl<T> Latest<T> {
    pub fn replace(&self, value: T) {
        match self.value.lock() {
            Ok(mut slot) => {
                *slot = Some(value);
            }
            Err(error) => {
                eprintln!("replay tools latest-state lock poisoned: {error}");
                *error.into_inner() = Some(value);
            }
        }
    }
    #[must_use]
    pub fn take(&self) -> Option<T> {
        match self.value.lock() {
            Ok(mut slot) => slot.take(),
            Err(error) => {
                eprintln!("replay tools latest-state lock poisoned: {error}");
                error.into_inner().take()
            }
        }
    }
}
#[derive(Default)]
pub struct UpdateThrottle {
    last_update_s: f64,
}
impl UpdateThrottle {
    pub fn accepts(&mut self, now: f64) -> bool {
        if now - self.last_update_s < 0.1 {
            false
        } else {
            self.last_update_s = now;
            true
        }
    }
}

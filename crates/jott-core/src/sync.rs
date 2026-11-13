//! Locking that does not turn one bug into an outage.

/// Take a `Mutex`, and carry on if the previous holder panicked.
///
/// The default `.unwrap()` propagates the panic to whoever next touches the lock, which turns one thread's bug
/// into a broken app — in our case usually through the *diagnostics*: the journal's own rule is that a log must
/// never be able to stop a save (`diagnostics.rs`), and `lock().unwrap()` contradicts it. A poisoned mutex still
/// holds structurally valid data here (a `VecDeque` of trace rows, an `Option<Writer>`, a stamp map); the panic
/// that poisoned it has already been reported at its own site, and adding "and now every later save fails" to it
/// loses the second failure and the notes with it.
///
/// This is not a general "ignore poisoning" policy for the crate: anything whose invariant a panic could leave
/// half-written should keep `.unwrap()` and stay broken loudly. It is used on the paths that must keep working —
/// logging, the trace, the tree's stamps — and nowhere else.
pub fn guarded<'a, T>(lock: &'a std::sync::Mutex<T>) -> std::sync::MutexGuard<'a, T> {
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::guarded;
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_poisoned_lock_is_still_usable() {
        let shared = Arc::new(Mutex::new(1usize));
        let holder = Arc::clone(&shared);
        // Poison it the way a real bug does: panic while holding it.
        let worker = std::thread::spawn(move || {
            let _guard = holder.lock().unwrap();
            panic!("a worker died in here");
        });
        let _ = worker.join();
        assert!(shared.is_poisoned(), "the setup worked: the lock is poisoned");

        *guarded(&shared) += 1;
        assert_eq!(*guarded(&shared), 2, "and the value is still readable and writable");
    }
}

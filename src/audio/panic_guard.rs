//! Turn panics inside third-party decoders into ordinary errors.
//!
//! Some decoders panic on input they cannot handle (for example rodio asserts
//! "unreachable" on a streamed fragmented MP4). A panic on the connection
//! worker would leave the engine waiting for an event that never comes, and
//! the app's panic hook would print "PulseDeck will close" over the TUI even
//! though the program carries on. [`run_quiet`] catches the panic and keeps the
//! hook silent for it.

use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};

thread_local! {
    static QUIET_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// True while the current thread is inside [`run_quiet`]. The panic hook uses
/// this to stay silent about panics that are caught and reported as errors.
pub fn quiet_panics_on_this_thread() -> bool {
    QUIET_DEPTH.with(|depth| depth.get() > 0)
}

/// Run `f`, returning `Err(())` if it panicked. Nested calls are fine.
pub(super) fn run_quiet<T>(f: impl FnOnce() -> T) -> Result<T, ()> {
    QUIET_DEPTH.with(|depth| depth.set(depth.get() + 1));
    let result = catch_unwind(AssertUnwindSafe(f));
    QUIET_DEPTH.with(|depth| depth.set(depth.get() - 1));
    result.map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_value_when_nothing_panics() {
        assert_eq!(run_quiet(|| 7), Ok(7));
        assert!(!quiet_panics_on_this_thread());
    }

    #[test]
    fn catches_a_panic_and_resets_the_flag() {
        let result = run_quiet(|| -> u8 { panic!("decoder exploded") });

        assert_eq!(result, Err(()));
        assert!(!quiet_panics_on_this_thread());
    }

    #[test]
    fn is_quiet_exactly_while_the_closure_runs() {
        assert!(!quiet_panics_on_this_thread());
        let inside = run_quiet(quiet_panics_on_this_thread).unwrap();
        assert!(inside);
        assert!(!quiet_panics_on_this_thread());
    }

    #[test]
    fn nested_calls_keep_the_outer_scope_quiet() {
        let outer = run_quiet(|| {
            let inner_result = run_quiet(|| -> u8 { panic!("inner") });
            (inner_result, quiet_panics_on_this_thread())
        })
        .unwrap();

        assert_eq!(outer, (Err(()), true));
        assert!(!quiet_panics_on_this_thread());
    }

    #[test]
    fn the_flag_is_per_thread() {
        let other = run_quiet(|| {
            std::thread::spawn(quiet_panics_on_this_thread)
                .join()
                .unwrap()
        })
        .unwrap();

        assert!(!other);
    }
}

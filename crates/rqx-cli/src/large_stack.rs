//! Runs the program on a thread whose stack is the same size on every platform.
//!
//! Windows gives the main thread 1 MB, which a debug build overflows at startup, where
//! macOS and Linux give 8 MB. The body runs on a thread with 8 MB everywhere, so the
//! program behaves the same on all of them.

const STACK_BYTES: usize = 8 << 20;

/// What a panicking `main` exits with in Rust, and what the documentation promises.
const PANIC_EXIT_CODE: i32 = 101;

/// Runs `body` on a thread with [`STACK_BYTES`] and returns its exit code. The thread is
/// named `main`, so a panic prints the message it always has: the panic hook prints it
/// on the panicking thread, and all that is left here is the exit code.
pub fn run(body: fn() -> i32) -> i32 {
    let spawned = std::thread::Builder::new()
        .name("main".into())
        .stack_size(STACK_BYTES)
        .spawn(body);
    match spawned {
        Ok(handle) => handle.join().unwrap_or(PANIC_EXIT_CODE),
        // No thread to be had: the main thread's own stack is the only one left.
        Err(_) => body(),
    }
}

#[cfg(test)]
mod tests {
    use super::{PANIC_EXIT_CODE, STACK_BYTES, run};

    #[test]
    fn the_exit_code_of_the_body_is_the_exit_code_of_the_process() {
        assert_eq!(run(|| 0), 0);
        assert_eq!(run(|| 1), 1);
        assert_eq!(run(|| 2), 2);
    }

    #[test]
    fn a_panic_in_the_body_exits_with_101() {
        assert_eq!(PANIC_EXIT_CODE, 101);
        assert_eq!(run(|| panic!("a bug, on purpose")), 101);
    }

    /// The test harness gives its threads 2 MB, so a body that needs more than that
    /// only passes because it runs on the thread `run` made.
    #[test]
    fn the_body_has_more_stack_than_a_test_thread() {
        fn four_megabytes_deep() -> i32 {
            let buffer = [1u8; 4 << 20];
            std::hint::black_box(&buffer);
            assert!(buffer.len() < STACK_BYTES);
            0
        }
        assert_eq!(run(four_megabytes_deep), 0);
    }
}

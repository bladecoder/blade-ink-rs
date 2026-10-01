//! Public Ink story API and shared runtime contracts.

pub use crate::story_player::Story;

pub mod errors;
pub mod external_functions;
pub mod variable_observer;

/// Supplies elapsed time to time-limited story continuation.
pub trait TimeSource {
    /// Returns a monotonically increasing duration.
    fn now(&self) -> core::time::Duration;
}

impl<F> TimeSource for F
where
    F: Fn() -> core::time::Duration,
{
    fn now(&self) -> core::time::Duration {
        self()
    }
}

/// The current version of the Ink story file format.
pub const INK_VERSION_CURRENT: i32 = 21;
/// The minimum Ink version accepted by the runtime.
pub const INK_VERSION_MINIMUM_COMPATIBLE: i32 = 18;

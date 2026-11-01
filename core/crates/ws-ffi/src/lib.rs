//! Synchronous C ABI v11; see ADRs 0018/0030/0031/0034/0038/0039/0040 and this crate's README for ownership.
#![allow(unsafe_code)] // The only native-pointer boundary in the workspace.

use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
};

/// Exact native/managed contract version.
pub const ABI_VERSION: u32 = 11;
mod slice_bridge;
pub use slice_bridge::{
    WsSliceConfig, WsSliceFaith, WsSliceGoal, WsSliceGod, WsSliceHandle, WsSliceOperation,
    ws_slice_apply, ws_slice_consequences, ws_slice_create, ws_slice_destroy,
    ws_slice_fill_geometry, ws_slice_fill_snapshot, ws_slice_get_calendar_sky, ws_slice_god,
    ws_slice_inspect, ws_slice_last_error, ws_slice_last_root,
};
mod calendar;
pub use calendar::{WsCalendarSky, WsUtcLabel, ws_utc_to_sim};
/// Fixed-width C status code; numeric values retain their ABI-v1 meanings.
pub type WsStatus = u32;
/// Successful operation.
pub const OK: u32 = 0;
/// Null pointer, invalid configuration or excessive step request.
pub const INVALID_ARGUMENT: u32 = 1;
/// Incompatible ABI or transport version.
pub const VERSION_MISMATCH: u32 = 2;
/// Caller must provide the required output capacity.
pub const BUFFER_TOO_SMALL: u32 = 3;
/// Malformed or unsupported command batch.
pub const INVALID_COMMAND: u32 = 4;
/// Boundary processing or a tick failed; earlier committed work remains applied.
pub const STEP_FAILED: u32 = 5;
/// Caught native unwind; handle is poisoned.
pub const PANIC: u32 = 6;
/// Only diagnostics and destruction are allowed on this handle.
pub const POISONED: u32 = 7;
/// A background save operation is still in progress.
pub const SAVE_IN_PROGRESS: u32 = 8;
#[derive(Clone)]
struct Diagnostic {
    bytes: [u8; 256],
    len: usize,
}
impl Diagnostic {
    const fn new() -> Self {
        Self {
            bytes: [0; 256],
            len: 0,
        }
    }
    fn set(&mut self, text: &str) {
        let mut n = text.len().min(255);
        while !text.is_char_boundary(n) {
            n -= 1;
        }
        self.bytes[..n].copy_from_slice(&text.as_bytes()[..n]);
        self.len = n;
    }
}
thread_local! { static LAST_ERROR: RefCell<Diagnostic> = const { RefCell::new(Diagnostic::new()) }; }
fn thread_error(message: &str) {
    LAST_ERROR.with(|e| e.borrow_mut().set(message));
}

/// Returns ABI version without creating a world.
#[unsafe(no_mangle)]
pub extern "C" fn ws_abi_version() -> u32 {
    ABI_VERSION
}

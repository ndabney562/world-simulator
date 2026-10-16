//! Synchronous C ABI v1; see ADR 0018 and this crate's README for ownership.
#![allow(unsafe_code)] // The only native-pointer boundary in the workspace.

use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    slice,
};
use ws_sim::{SimTime, World, WorldConfig};

/// Exact native/managed contract version.
pub const ABI_VERSION: u32 = 1;
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
/// A tick failed; earlier successful ticks remain applied.
pub const STEP_FAILED: u32 = 5;
/// Caught native unwind; handle is poisoned.
pub const PANIC: u32 = 6;
/// Only diagnostics and destruction are allowed on this handle.
pub const POISONED: u32 = 7;
/// Snapshot header bytes.
pub const HEADER_SIZE: usize = 24;
/// Snapshot instance record bytes.
pub const RECORD_SIZE: usize = 32;

/// Creation configuration, 32 bytes aligned to 8 on supported targets.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WsConfig {
    /// Must equal 1.
    pub abi_version: u32,
    /// Must equal 32.
    pub struct_size: u32,
    /// Root seed.
    pub seed: u64,
    /// Epoch-relative microseconds, divisible by 100,000.
    pub start_us: i64,
    /// Debug markers, 0 through 100,000; not canonical entities.
    pub debug_marker_count: u32,
    /// Must be zero.
    pub reserved: u32,
}

/// Snapshot v1 header, serialized little-endian without alignment assumptions.
#[repr(C)]
pub struct WsSnapshotHeader {
    /// ABI version.
    pub abi_version: u32,
    /// Header bytes (24).
    pub header_size: u32,
    /// Completed base steps.
    pub tick: u64,
    /// Instance count.
    pub instance_count: u32,
    /// Instance record bytes (32).
    pub record_size: u32,
}
/// Debug render record, never a canonical entity.
#[repr(C)]
pub struct WsInstance {
    /// Debug ID, index plus one.
    pub id: u64,
    /// Zero for debug markers.
    pub kind: u32,
    /// East metres.
    pub east: f32,
    /// North metres.
    pub north: f32,
    /// Up metres.
    pub up: f32,
    /// Heading radians from east towards north.
    pub heading: f32,
    /// Zero.
    pub reserved: u32,
}

/// Opaque C handle. Never construct, dereference or copy its storage in a caller.
pub struct WsHandle {
    world: World,
    markers: u32,
    error: Diagnostic,
    poisoned: bool,
}

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

/// Creates a world; null on invalid config or caught panic. Read last_error(NULL).
/// # Safety
/// Non-null config must point to one aligned, initialized WsConfig for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_create(config: *const WsConfig) -> *mut WsHandle {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() {
            return Err("config is null");
        }
        // SAFETY: caller guarantees a readable aligned config; no pointer is retained.
        let config = unsafe { *config };
        if config.abi_version != ABI_VERSION {
            return Err("ABI version mismatch");
        }
        if config.struct_size != 32 || config.reserved != 0 || config.debug_marker_count > 100_000 {
            return Err("invalid config size, reserved field or marker count");
        }
        let world = World::new(WorldConfig::new(
            config.seed,
            SimTime::from_micros_since_epoch(config.start_us),
        ))
        .map_err(|_| "world creation failed (check start alignment)")?;
        Ok(Box::into_raw(Box::new(WsHandle {
            world,
            markers: config.debug_marker_count,
            error: Diagnostic::new(),
            poisoned: false,
        })))
    }));
    match result {
        Ok(Ok(handle)) => {
            thread_error("");
            handle
        }
        Ok(Err(message)) => {
            thread_error(message);
            std::ptr::null_mut()
        }
        Err(_) => {
            thread_error("panic during creation");
            std::ptr::null_mut()
        }
    }
}

/// Destroys exactly one live handle; null is a no-op.
/// # Safety
/// Non-null handle must be from create, live, exclusively owned and not destroyed before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_destroy(handle: *mut WsHandle) {
    if handle.is_null() {
        return;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: caller transfers the unique Box allocation back exactly once.
        drop(unsafe { Box::from_raw(handle) });
    }));
    if result.is_err() {
        thread_error("panic during destruction");
    }
}

unsafe fn operate(
    handle: *mut WsHandle,
    f: impl FnOnce(&mut WsHandle) -> Result<(), (u32, &'static str)>,
) -> u32 {
    if handle.is_null() {
        thread_error("handle is null");
        return INVALID_ARGUMENT;
    }
    // SAFETY: all exported callers require a live handle exclusively borrowed for the call.
    let handle = unsafe { &mut *handle };
    if handle.poisoned {
        handle.error.set("handle is poisoned");
        return POISONED;
    }
    match catch_unwind(AssertUnwindSafe(|| f(handle))) {
        Ok(Ok(())) => {
            handle.error.set("");
            OK
        }
        Ok(Err((status, message))) => {
            handle.error.set(message);
            status
        }
        Err(_) => {
            handle.poisoned = true;
            handle.error.set("native panic; destroy this handle");
            PANIC
        }
    }
}

fn validate_commands(bytes: &[u8]) -> Result<(), (u32, &'static str)> {
    let invalid = (INVALID_COMMAND, "invalid NoOp batch");
    if bytes.len() < 8 || bytes.len() > 65_536 {
        return Err(invalid);
    }
    let read = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    if read(0) != 1 {
        return Err((VERSION_MISMATCH, "command version mismatch"));
    }
    let count = read(4) as usize;
    if count > 1_024 || bytes.len() != 8 + count * 8 {
        return Err(invalid);
    }
    for index in 0..count {
        let offset = 8 + index * 8;
        if read(offset) != 0 || read(offset + 4) != 0 {
            return Err(invalid);
        }
    }
    Ok(())
}

/// Validates an ABI-v1 batch of NoOps; copies/retains no bytes and changes no state.
/// # Safety
/// Handle must be live/exclusive; positive len needs readable bytes for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_submit(handle: *mut WsHandle, bytes: *const u8, len: usize) -> u32 {
    // SAFETY: caller upholds handle and buffer lifetime; lengths checked before borrowing.
    unsafe {
        operate(handle, |_| {
            if len > 65_536 || len == 0 {
                return Err((INVALID_COMMAND, "invalid batch size"));
            }
            if bytes.is_null() {
                return Err((INVALID_ARGUMENT, "batch pointer is null"));
            }
            validate_commands(slice::from_raw_parts(bytes, len))
        })
    }
}

/// Advances whole base steps (0..100); stops at the first failed tick.
/// # Safety
/// Handle must be live and exclusively borrowed for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_advance(handle: *mut WsHandle, steps: u32) -> u32 {
    // SAFETY: caller upholds the handle contract.
    unsafe {
        operate(handle, |h| {
            if steps > 100 {
                return Err((INVALID_ARGUMENT, "at most 100 steps per call"));
            }
            for _ in 0..steps {
                h.world.step().map_err(|_| (STEP_FAILED, "tick failed"))?;
            }
            Ok(())
        })
    }
}

/// Fills a coherent snapshot; undersized buffers are untouched and written gives required bytes.
/// # Safety
/// Handle must be live/exclusive. Written must be aligned writable usize storage, disjoint
/// from buf/handle; non-null buf must be writable for cap bytes without other access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_fill_render_snapshot(
    handle: *mut WsHandle,
    buf: *mut u8,
    cap: usize,
    written: *mut usize,
) -> u32 {
    if written.is_null() {
        thread_error("written is null");
        return INVALID_ARGUMENT;
    }
    // SAFETY: caller provides writable aligned output storage, disjoint from all other inputs.
    unsafe {
        *written = 0;
    }
    // SAFETY: caller upholds the handle/output contracts; no caller memory is retained.
    unsafe {
        operate(handle, |h| {
            let required = HEADER_SIZE + h.markers as usize * RECORD_SIZE;
            if cap < required {
                *written = required;
                return Err((BUFFER_TOO_SMALL, "snapshot capacity too small"));
            }
            if buf.is_null() {
                return Err((INVALID_ARGUMENT, "snapshot buffer is null"));
            }
            let out = slice::from_raw_parts_mut(buf, required);
            out[..4].copy_from_slice(&ABI_VERSION.to_le_bytes());
            out[4..8].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
            out[8..16].copy_from_slice(&h.world.tick().to_le_bytes());
            out[16..20].copy_from_slice(&h.markers.to_le_bytes());
            out[20..24].copy_from_slice(&(RECORD_SIZE as u32).to_le_bytes());
            for index in 0..h.markers {
                let record = &mut out[HEADER_SIZE + index as usize * RECORD_SIZE..][..RECORD_SIZE];
                record.fill(0);
                record[..8].copy_from_slice(&(u64::from(index) + 1).to_le_bytes());
                let marker = ws_sim::debug_marker(h.world.seed(), h.world.tick(), index);
                for (field, value) in marker.into_iter().enumerate() {
                    record[12 + field * 4..16 + field * 4].copy_from_slice(&value.to_le_bytes());
                }
            }
            *written = required;
            Ok(())
        })
    }
}

/// Copies bounded UTF-8 diagnostic; required length includes NUL. Null handle reads thread-local error.
/// # Safety
/// Non-null handle must be live/exclusive; non-null buf must be writable for cap bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_last_error(
    handle: *mut WsHandle,
    buf: *mut std::ffi::c_char,
    cap: usize,
) -> usize {
    if cap > 0 && buf.is_null() {
        return 0;
    }
    let copy = |error: &Diagnostic| {
        if cap > 0 {
            let mut n = error.len.min(cap - 1);
            while std::str::from_utf8(&error.bytes[..n]).is_err() {
                n -= 1;
            }
            // SAFETY: caller provides cap writable bytes; n < cap, and buffers do not overlap.
            unsafe {
                std::ptr::copy_nonoverlapping(error.bytes.as_ptr(), buf.cast(), n);
                *buf.add(n) = 0;
            }
        }
        error.len + 1
    };
    if handle.is_null() {
        LAST_ERROR.with(|error| copy(&error.borrow()))
    } else {
        // SAFETY: caller upholds live/exclusive handle requirement.
        copy(unsafe { &(*handle).error })
    }
}

#[cfg(test)]
mod tests;

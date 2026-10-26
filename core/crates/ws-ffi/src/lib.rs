//! Synchronous C ABI v8; see ADRs 0018/0019/0022/0030/0031 and this crate's README for ownership.
#![allow(unsafe_code)] // The only native-pointer boundary in the workspace.

use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    slice,
};
use ws_sim::{
    Population, RenderInstance, SimTime, StepError, World, WorldConfig, commands::IngressError,
};

/// Exact native/managed contract version.
pub const ABI_VERSION: u32 = 8;
mod slice_bridge;
pub use slice_bridge::{
    WsSliceConfig, WsSliceHandle, WsSliceOperation, ws_slice_apply, ws_slice_consequences,
    ws_slice_create, ws_slice_destroy, ws_slice_fill_geometry, ws_slice_fill_snapshot,
    ws_slice_get_calendar_sky, ws_slice_inspect, ws_slice_last_error,
};
mod calendar;
pub use calendar::{WsCalendarSky, WsUtcLabel, ws_get_calendar_sky, ws_utc_to_sim};
/// Most residents per building a config may ask for.
pub const MAX_RESIDENTS_PER_BUILDING: u32 = 2_000;
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
/// Snapshot header bytes.
pub const HEADER_SIZE: usize = 24;
/// Snapshot instance record bytes.
pub const RECORD_SIZE: usize = 32;

/// Creation configuration, 32 bytes aligned to 8 on supported targets.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WsConfig {
    /// Must equal 5.
    pub abi_version: u32,
    /// Must equal 32.
    pub struct_size: u32,
    /// Root seed.
    pub seed: u64,
    /// Epoch-relative microseconds, divisible by 100,000.
    pub start_us: i64,
    /// Residents in every building, 1 through 2,000 (the toy world has five buildings).
    pub residents_per_building: u32,
    /// Must be zero.
    pub reserved: u32,
}

/// Coarse committed control state; 16 bytes, aligned to 4. Input: version and size only.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WsControlState {
    /// Caller requests ABI5; returned as5 on success.
    pub abi_version: u32,
    /// Caller provides 16 bytes; returned as 16 on success.
    pub struct_size: u32,
    /// Authoritative committed scale: 0, 1, 10 or 100.
    pub scale_tag: u32,
    /// Returned as zero.
    pub reserved: u32,
}

/// Snapshot v5 header, serialized little-endian without alignment assumptions.
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
/// Render record (ADR 0022): kind 1 is a building box, kind 2 a resident capsule.
#[repr(C)]
pub struct WsInstance {
    /// Raw entity ID.
    pub id: u64,
    /// 1 building, 2 resident.
    pub kind: u32,
    /// East metres of the snapshot origin.
    pub east: f32,
    /// North metres of the snapshot origin.
    pub north: f32,
    /// Up metres of the snapshot origin.
    pub up: f32,
    /// Building half extent in metres, or resident heading in radians.
    pub a: f32,
    /// Building height in metres, or zero.
    pub b: f32,
}

/// Opaque C handle. Never construct, dereference or copy its storage in a caller.
pub struct WsHandle {
    world: World,
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
        if config.struct_size != 32
            || config.reserved != 0
            || !(1..=MAX_RESIDENTS_PER_BUILDING).contains(&config.residents_per_building)
        {
            return Err("invalid config size, reserved field or residents per building");
        }
        let mut world_config = WorldConfig::new(
            config.seed,
            SimTime::from_micros_since_epoch(config.start_us),
        );
        world_config.population = Population {
            min_per_building: config.residents_per_building,
            max_per_building: config.residents_per_building,
        };
        let world = World::new(world_config)
            .map_err(|_| "world creation failed (check start alignment)")?;
        Ok(Box::into_raw(Box::new(WsHandle {
            world,
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

fn ingress_error(error: IngressError) -> (u32, &'static str) {
    match error {
        IngressError::Version => (VERSION_MISMATCH, "command version mismatch"),
        IngressError::Malformed => (INVALID_COMMAND, "malformed command batch"),
        IngressError::Capacity => (
            INVALID_COMMAND,
            "command batch or pending queue capacity exceeded",
        ),
        IngressError::Resource => (STEP_FAILED, "command queue reservation failed"),
    }
}

/// Atomically copies a bounded v2 batch into the journaled queue; success means pending.
/// # Safety
/// Handle must be live/exclusive; positive len needs readable bytes for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_submit(
    handle: *mut WsHandle,
    bytes: *const u8,
    len: usize,
) -> WsStatus {
    // SAFETY: caller upholds handle and buffer lifetime; lengths checked before borrowing.
    unsafe {
        operate(handle, |h| {
            if bytes.is_null() && len > 0 {
                return Err((INVALID_ARGUMENT, "batch pointer is null"));
            }
            if !(8..=65_536).contains(&len) {
                return Err((INVALID_COMMAND, "invalid batch size"));
            }
            h.world
                .submit_bytes(slice::from_raw_parts(bytes, len))
                .map(|_| ())
                .map_err(ingress_error)
        })
    }
}

/// Flushes once, then advances explicit whole steps (0..100), independent of scale.
/// Zero steps flush even while paused. Invalid step counts fail before flushing.
/// # Safety
/// Handle must be live and exclusively borrowed for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_advance(handle: *mut WsHandle, steps: u32) -> WsStatus {
    // SAFETY: caller upholds the handle contract.
    unsafe {
        operate(handle, |h| {
            if steps > 100 {
                return Err((INVALID_ARGUMENT, "at most 100 steps per call"));
            }
            h.world
                .flush_commands()
                .map_err(|_| (STEP_FAILED, "command boundary processing failed"))?;
            for _ in 0..steps {
                h.world.physical_step().map_err(|error| {
                    (
                        STEP_FAILED,
                        match error {
                            StepError::Time(_) => "tick failed: simulation time overflow",
                            StepError::World(_) => "tick failed: world system error",
                            StepError::Control(_) => "tick failed: command processing error",
                            StepError::History(_) => "tick failed: history publication error",
                        },
                    )
                })?;
            }
            Ok(())
        })
    }
}

/// Reads committed control state without flushing, stepping or publishing events.
/// Errors leave the destination unchanged and use the handle diagnostic when usable.
/// # Safety
/// Handle must be live/exclusive. A non-null out_state must be aligned writable
/// storage for 16 bytes, disjoint from handle; its first two u32 fields must be initialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_get_control_state(
    handle: *mut WsHandle,
    out_state: *mut WsControlState,
) -> WsStatus {
    // SAFETY: caller guarantees handle and output storage; validate before any write.
    unsafe {
        operate(handle, |h| {
            if out_state.is_null() || !out_state.is_aligned() {
                return Err((
                    INVALID_ARGUMENT,
                    "control-state output is null or unaligned",
                ));
            }
            // Read only the initialized request fields, not the output-only fields.
            if std::ptr::addr_of!((*out_state).abi_version).read() != ABI_VERSION {
                return Err((VERSION_MISMATCH, "control-state ABI version mismatch"));
            }
            if std::ptr::addr_of!((*out_state).struct_size).read() != 16 {
                return Err((INVALID_ARGUMENT, "control-state size must be 16"));
            }
            out_state.write(WsControlState {
                abi_version: ABI_VERSION,
                struct_size: 16,
                scale_tag: h.world.clock().scale().multiplier(),
                reserved: 0,
            });
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
) -> WsStatus {
    if !written.is_null() {
        // SAFETY: caller provides writable aligned output storage, disjoint from all other inputs.
        unsafe {
            *written = 0;
        }
    }
    // SAFETY: caller upholds the handle/output contracts; no caller memory is retained.
    unsafe {
        operate(handle, |h| {
            if written.is_null() {
                return Err((INVALID_ARGUMENT, "written is null"));
            }
            let count = h.world.render_instance_count();
            let required = HEADER_SIZE + count * RECORD_SIZE;
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
            out[16..20].copy_from_slice(&(count as u32).to_le_bytes());
            out[20..24].copy_from_slice(&(RECORD_SIZE as u32).to_le_bytes());
            let mut at = HEADER_SIZE;
            h.world.render_instances(|i: RenderInstance| {
                let record = &mut out[at..at + RECORD_SIZE];
                record[..8].copy_from_slice(&i.id.raw().to_le_bytes());
                record[8..12].copy_from_slice(&(i.kind as u32).to_le_bytes());
                for (n, v) in [i.east, i.north, i.up, i.a, i.b].into_iter().enumerate() {
                    record[12 + n * 4..16 + n * 4].copy_from_slice(&v.to_le_bytes());
                }
                at += RECORD_SIZE;
            });
            debug_assert_eq!(at, required);
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

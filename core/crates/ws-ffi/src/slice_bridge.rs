//! ADR0031 separate real-slice ownership and bounded coarse ABI6 transport.
use super::*;
mod inspection;
pub use inspection::ws_slice_inspect;
use ws_sim::slice::{Operation, SliceConfig, SliceSimulation, render};

const GEOMETRY_HEADER: usize = 96;
const SNAPSHOT_HEADER: usize = 128;
const ROW: usize = 48;
const MAX_POPULATION: usize = 30;
const SUMMARY_COUNT: usize = 4;
const MAX_DYNAMIC: usize = SNAPSHOT_HEADER + (MAX_POPULATION + SUMMARY_COUNT) * ROW;

/// Real-slice creation,32 bytes aligned8. Population is the pinned synthetic cohort.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WsSliceConfig {
    /// Exact current native ABI6.
    pub abi_version: u32,
    /// Exactly32.
    pub struct_size: u32,
    /// Procedural seed, retaining the existing seed-generated10–30 residents.
    pub seed: u64,
    /// Configured100-ms-grid SimTime; creation must be in UTC2000–2099.
    pub start_us: i64,
    /// Zero.
    pub reserved: u64,
}
/// One coarse input,64 bytes aligned8; no per-entity operation is exposed here.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WsSliceOperation {
    /// Exact current ABI6.
    pub abi_version: u32,
    /// Exactly64.
    pub struct_size: u32,
    /// 1steps,2scale,3host elapsed microseconds,4camera target/radius.
    pub tag: u32,
    /// Zero.
    pub reserved: u32,
    /// Tags1–3: unsigned word0 and remaining words0. Tag4: four finite f64
    /// bit patterns (ENU target metres,radius metres), last two words0.
    pub words: [u64; 6],
}
/// Opaque serialized real-slice owner. Never cast to/from the toy WsHandle.
pub struct WsSliceHandle {
    world: SliceSimulation,
    geometry: render::Geometry,
    geometry_bytes: Vec<u8>,
    error: Diagnostic,
    poisoned: bool,
    #[cfg(test)]
    panic_before_output: bool,
}
unsafe fn operate_slice(
    h: *mut WsSliceHandle,
    f: impl FnOnce(&mut WsSliceHandle) -> Result<(), (WsStatus, &'static str)>,
) -> WsStatus {
    if h.is_null() {
        thread_error("slice handle is null");
        return INVALID_ARGUMENT;
    }
    // SAFETY: caller guarantees live exclusive ownership of the correct opaque handle.
    let h = unsafe { &mut *h };
    if h.poisoned {
        h.error.set("slice handle is poisoned");
        return POISONED;
    }
    match catch_unwind(AssertUnwindSafe(|| f(h))) {
        Ok(Ok(())) => {
            h.error.set("");
            OK
        }
        Ok(Err((status, message))) => {
            h.error.set(message);
            status
        }
        Err(_) => {
            h.poisoned = true;
            h.error.set("native panic; destroy this slice handle");
            PANIC
        }
    }
}
fn u32_at(out: &mut [u8], at: usize, v: u32) {
    out[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn u64_at(out: &mut [u8], at: usize, v: u64) {
    out[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn f32_at(out: &mut [u8], at: usize, v: f32) {
    u32_at(out, at, v.to_bits());
}
fn f64_at(out: &mut [u8], at: usize, v: f64) {
    u64_at(out, at, v.to_bits());
}
fn encode_geometry(g: &render::Geometry) -> Result<Vec<u8>, &'static str> {
    if g.buildings.len() > render::MAX_BUILDINGS
        || g.vertices.len() > render::MAX_VERTICES
        || g.indices.len() > render::MAX_INDICES
    {
        return Err("slice geometry exceeds transport limits");
    }
    let len =
        GEOMETRY_HEADER + g.buildings.len() * ROW + g.vertices.len() * 24 + g.indices.len() * 4;
    let mut out = Vec::new();
    out.try_reserve_exact(len)
        .map_err(|_| "geometry byte reservation failed")?;
    out.resize(len, 0);
    u32_at(&mut out, 0, ABI_VERSION);
    u32_at(&mut out, 4, GEOMETRY_HEADER as u32);
    u64_at(&mut out, 8, ws_sim::SITE_DATASET_VERSION);
    for (at, v) in [
        (16, render::RENDER_VERSION),
        (20, g.buildings.len() as u32),
        (24, g.vertices.len() as u32),
        (28, g.indices.len() as u32),
        (32, 48),
        (36, 24),
        (40, 4),
    ] {
        u32_at(&mut out, at, v);
    }
    for (i, v) in render::frame_origin().into_iter().enumerate() {
        f64_at(&mut out, 48 + i * 8, v);
    }
    for (i, v) in g.bounds.into_iter().enumerate() {
        f32_at(&mut out, 72 + i * 4, v);
    }
    let mut at = GEOMETRY_HEADER;
    for b in &g.buildings {
        u64_at(&mut out, at, b.bin);
        u64_at(&mut out, at + 8, b.entity.map_or(0, |v| v.raw()));
        for (offset, v) in [
            (16, b.first_vertex),
            (20, b.vertex_count),
            (24, b.first_index),
            (28, b.index_count),
        ] {
            u32_at(&mut out, at + offset, v);
        }
        f32_at(&mut out, at + 32, b.height_m);
        u32_at(&mut out, at + 36, u32::from(b.entity.is_some()));
        f32_at(&mut out, at + 40, b.centroid[0]);
        f32_at(&mut out, at + 44, b.centroid[1]);
        at += ROW;
    }
    for v in &g.vertices {
        for (i, value) in v.position.into_iter().chain(v.normal).enumerate() {
            f32_at(&mut out, at + i * 4, value);
        }
        at += 24;
    }
    for &index in &g.indices {
        u32_at(&mut out, at, index);
        at += 4;
    }
    debug_assert_eq!(at, len);
    Ok(out)
}
fn encode_snapshot(
    s: &render::Snapshot,
    out: &mut [u8; MAX_DYNAMIC],
) -> Result<usize, (WsStatus, &'static str)> {
    if s.residents.len() != s.summary.population as usize || s.residents.len() > MAX_POPULATION {
        return Err((STEP_FAILED, "invalid slice population view"));
    }
    u32_at(out, 0, ABI_VERSION);
    u32_at(out, 4, SNAPSHOT_HEADER as u32);
    u64_at(out, 8, s.tick);
    u64_at(out, 16, s.time.as_micros_since_epoch() as u64);
    u64_at(out, 24, s.state_hash);
    u64_at(out, 32, s.dataset);
    for (at, v) in [
        (40, render::RENDER_VERSION),
        (44, s.scale),
        (48, s.summary.population),
        (52, s.residents.len() as u32),
        (56, SUMMARY_COUNT as u32),
        (60, 48),
        (64, 48),
        (72, s.summary.local_stationary),
        (76, s.summary.in_transit),
        (80, s.summary.off_map_stationary),
        (84, s.summary.l1),
        (88, s.summary.l2),
    ] {
        u32_at(out, at, v);
    }
    let mut at = SNAPSHOT_HEADER;
    for r in &s.residents {
        if !r.position.iter().all(|v| v.is_finite()) || !r.heading.is_finite() {
            return Err((STEP_FAILED, "nonfinite slice render row"));
        }
        u64_at(out, at, r.id.raw());
        u64_at(out, at + 8, r.governing.raw());
        for (i, v) in r.position.into_iter().enumerate() {
            f32_at(out, at + 16 + i * 4, v);
        }
        f32_at(out, at + 28, r.heading);
        u32_at(out, at + 32, r.detail);
        u32_at(out, at + 36, r.activity);
        u32_at(out, at + 40, r.location);
        at += ROW;
    }
    for b in s.summary.buildings {
        u64_at(out, at, b.building.raw());
        u32_at(out, at + 8, b.mode as u32);
        u32_at(out, at + 12, b.occupancy);
        u32_at(out, at + 16, b.governed);
        for (i, v) in b.activities.into_iter().enumerate() {
            u32_at(out, at + 20 + i * 4, v);
        }
        at += ROW;
    }
    Ok(at)
}

/// Creates an explicit real slice, null on validation/construction failure or panic.
/// Creation uses the stored calendar-aligned Monday epoch; no wall clock is read.
/// # Safety
/// Non-null config points to one readable aligned initialized WsSliceConfig.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_slice_create(config: *const WsSliceConfig) -> *mut WsSliceHandle {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() || !config.is_aligned() {
            return Err("slice config is null or unaligned");
        }
        // SAFETY: caller supplies readable initialized configuration, not retained.
        let c = unsafe { config.read() };
        if c.abi_version != ABI_VERSION {
            return Err("slice ABI version mismatch");
        }
        if c.struct_size != 32 || c.reserved != 0 {
            return Err("slice config size/reserved invalid");
        }
        let config =
            SliceConfig::calendar_aligned_v1(c.seed, SimTime::from_micros_since_epoch(c.start_us))
                .map_err(|_| "unsupported slice calendar start")?;
        let world = SliceSimulation::new(config)
            .map_err(|_| "slice creation failed (check100-ms start grid)")?;
        if world.population().residents().len() > MAX_POPULATION
            || world.population().world().buildings().len() != SUMMARY_COUNT
        {
            return Err("unsupported slice cohort");
        }
        let geometry = world
            .render_geometry()
            .map_err(|_| "slice source geometry construction failed")?;
        let geometry_bytes = encode_geometry(&geometry)?;
        Ok(Box::into_raw(Box::new(WsSliceHandle {
            world,
            geometry,
            geometry_bytes,
            error: Diagnostic::new(),
            poisoned: false,
            #[cfg(test)]
            panic_before_output: false,
        })))
    }));
    match result {
        Ok(Ok(h)) => {
            thread_error("");
            h
        }
        Ok(Err(message)) => {
            thread_error(message);
            std::ptr::null_mut()
        }
        Err(_) => {
            thread_error("panic during slice creation");
            std::ptr::null_mut()
        }
    }
}
/// Destroys exactly one live slice handle; null is a no-op.
/// # Safety
/// Non-null handle is live, exclusively owned and not destroyed before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_slice_destroy(handle: *mut WsSliceHandle) {
    if handle.is_null() {
        return;
    }
    if catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: caller transfers the original unique allocation back once.
        drop(unsafe { Box::from_raw(handle) });
    }))
    .is_err()
    {
        thread_error("panic during slice destruction");
    }
}
fn operation(
    h: &WsSliceHandle,
    c: WsSliceOperation,
) -> Result<Operation, (WsStatus, &'static str)> {
    if c.abi_version != ABI_VERSION {
        return Err((VERSION_MISMATCH, "slice operation ABI mismatch"));
    }
    if c.struct_size != 64 || c.reserved != 0 {
        return Err((INVALID_ARGUMENT, "slice operation size/reserved invalid"));
    }
    if c.tag == 4 {
        if c.words[4..].iter().any(|v| *v != 0) {
            return Err((INVALID_COMMAND, "camera reserved words must be zero"));
        }
        let p = core::array::from_fn(|i| f64::from_bits(c.words[i]));
        return h
            .world
            .camera_operation(p, f64::from_bits(c.words[3]))
            .map_err(|_| (INVALID_ARGUMENT, "invalid slice camera input"));
    }
    if c.words[1..].iter().any(|v| *v != 0) {
        return Err((INVALID_COMMAND, "unused operation words must be zero"));
    }
    match c.tag {
        1 if c.words[0] <= 100 => Ok(Operation::Steps(c.words[0])),
        1 => Err((INVALID_ARGUMENT, "slice explicit steps exceed100")),
        2 => Ok(Operation::SetScale(match c.words[0] {
            0 => ws_sim::TimeScale::Paused,
            1 => ws_sim::TimeScale::X1,
            10 => ws_sim::TimeScale::X10,
            100 => ws_sim::TimeScale::X100,
            _ => return Err((INVALID_COMMAND, "invalid slice scale")),
        })),
        3 => Ok(Operation::HostElapsed(c.words[0])),
        _ => Err((INVALID_COMMAND, "unknown slice operation tag")),
    }
}
/// Validates then applies one coarse operation. Failure preserves the core's
/// committed prefix and recorded stop; query the snapshot for that boundary.
/// Wire validation failure performs no domain operation. No pointers are retained.
/// # Safety
/// Handle is live/exclusive; input points to one readable aligned initialized operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_slice_apply(
    handle: *mut WsSliceHandle,
    input: *const WsSliceOperation,
) -> WsStatus {
    // SAFETY: caller supplies serialized handle/input ownership; pointers checked before reading.
    unsafe {
        operate_slice(handle, |h| {
            if input.is_null() || !input.is_aligned() {
                return Err((INVALID_ARGUMENT, "slice operation is null or unaligned"));
            }
            let op = operation(h, input.read())?;
            let result = h
                .world
                .apply(op)
                .map_err(|_| (STEP_FAILED, "slice operation reservation failed"))?;
            if result.error.is_some() {
                return Err((
                    STEP_FAILED,
                    "slice operation stopped after committed prefix",
                ));
            }
            Ok(())
        })
    }
}
unsafe fn fill_bytes(
    bytes: &[u8],
    buffer: *mut u8,
    capacity: usize,
    written: *mut usize,
) -> Result<(), (WsStatus, &'static str)> {
    if written.is_null() || !written.is_aligned() {
        return Err((
            INVALID_ARGUMENT,
            "slice written pointer is null or unaligned",
        ));
    }
    if capacity < bytes.len() {
        // SAFETY: caller supplies writable aligned control output; data buffer stays untouched.
        unsafe {
            written.write(bytes.len());
        }
        return Err((BUFFER_TOO_SMALL, "slice output capacity too small"));
    }
    if buffer.is_null() {
        return Err((INVALID_ARGUMENT, "slice output buffer is null"));
    }
    // SAFETY: caller guarantees disjoint writable buffers; all fallible work finished.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
        written.write(bytes.len());
    }
    Ok(())
}
/// Copies the once-built static geometry. Short capacity reports required bytes
/// without touching data; other failures leave both output arguments unchanged.
/// # Safety
/// Handle is live/exclusive; non-null buffer is writable for capacity bytes;
/// written is aligned writable usize storage, disjoint from buffer/handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_slice_fill_geometry(
    handle: *mut WsSliceHandle,
    buffer: *mut u8,
    capacity: usize,
    written: *mut usize,
) -> WsStatus {
    // SAFETY: caller upholds serialized/disjoint output ownership.
    unsafe {
        operate_slice(handle, |h| {
            debug_assert_eq!(
                h.geometry_bytes.len(),
                GEOMETRY_HEADER
                    + h.geometry.buildings.len() * ROW
                    + h.geometry.vertices.len() * 24
                    + h.geometry.indices.len() * 4
            );
            #[cfg(test)]
            if h.panic_before_output {
                panic!("geometry output fault");
            }
            fill_bytes(&h.geometry_bytes, buffer, capacity, written)
        })
    }
}
/// Copies one pure complete committed resident/count snapshot. All serialization
/// finishes in bounded scratch storage before any caller data is written.
/// # Safety
/// Same live/exclusive/disjoint buffer contract as ws_slice_fill_geometry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_slice_fill_snapshot(
    handle: *mut WsSliceHandle,
    buffer: *mut u8,
    capacity: usize,
    written: *mut usize,
) -> WsStatus {
    // SAFETY: caller supplies exclusively owned handle and disjoint writable output.
    unsafe {
        operate_slice(handle, |h| {
            if written.is_null() || !written.is_aligned() {
                return Err((
                    INVALID_ARGUMENT,
                    "slice written pointer is null or unaligned",
                ));
            }
            let required =
                SNAPSHOT_HEADER + (h.world.population().residents().len() + SUMMARY_COUNT) * ROW;
            if capacity < required {
                written.write(required);
                return Err((BUFFER_TOO_SMALL, "slice snapshot capacity too small"));
            }
            if buffer.is_null() {
                return Err((INVALID_ARGUMENT, "slice output buffer is null"));
            }
            let snapshot = h
                .world
                .render_snapshot()
                .map_err(|_| (STEP_FAILED, "slice render query failed"))?;
            let mut scratch = [0u8; MAX_DYNAMIC];
            let len = encode_snapshot(&snapshot, &mut scratch)?;
            #[cfg(test)]
            if h.panic_before_output {
                panic!("snapshot output fault");
            }
            fill_bytes(&scratch[..len], buffer, capacity, written)
        })
    }
}
/// Pure160-byte slice sky query in the same fixed geometry basis. Caller initializes
/// only ABI/size; no output bytes change on validation error, poison or panic.
/// # Safety
/// Handle is live/exclusive; output is aligned writable160-byte storage disjoint
/// from the handle. Serialize across dynamic fill and this query before pairing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_slice_get_calendar_sky(
    handle: *mut WsSliceHandle,
    output: *mut WsCalendarSky,
) -> WsStatus {
    // SAFETY: caller guarantees handle/output ownership; only initialized header fields read.
    unsafe {
        operate_slice(handle, |h| {
            if output.is_null() || !output.is_aligned() {
                return Err((INVALID_ARGUMENT, "slice sky output is null or unaligned"));
            }
            if std::ptr::addr_of!((*output).abi_version).read() != ABI_VERSION {
                return Err((VERSION_MISMATCH, "slice sky ABI mismatch"));
            }
            if std::ptr::addr_of!((*output).struct_size).read() != 160 {
                return Err((INVALID_ARGUMENT, "slice sky size must be160"));
            }
            let value = WsCalendarSky::from_view(h.world.calendar_sky());
            #[cfg(test)]
            if h.panic_before_output {
                panic!("sky output fault");
            }
            output.write(value);
            Ok(())
        })
    }
}
/// Copies the bounded diagnostic, including on a poisoned handle. Null reads TLS.
/// # Safety
/// Non-null handle is live/exclusive; positive capacity requires a writable buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_slice_last_error(
    handle: *mut WsSliceHandle,
    buffer: *mut std::ffi::c_char,
    capacity: usize,
) -> usize {
    if capacity > 0 && buffer.is_null() {
        return 0;
    }
    let copy = |error: &Diagnostic| {
        if capacity > 0 {
            let mut n = error.len.min(capacity - 1);
            while std::str::from_utf8(&error.bytes[..n]).is_err() {
                n -= 1;
            }
            // SAFETY: caller guarantees capacity writable bytes, disjoint from diagnostic storage.
            unsafe {
                std::ptr::copy_nonoverlapping(error.bytes.as_ptr(), buffer.cast(), n);
                *buffer.add(n) = 0;
            }
        }
        error.len + 1
    };
    if handle.is_null() {
        LAST_ERROR.with(|e| copy(&e.borrow()))
    } else {
        // SAFETY: caller supplies the correct live serialized opaque handle.
        copy(unsafe { &(*handle).error })
    }
}

#[cfg(test)]
#[path = "slice_bridge/tests.rs"]
mod tests;

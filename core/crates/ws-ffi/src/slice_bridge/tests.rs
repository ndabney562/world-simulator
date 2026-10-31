use super::*;
use std::ptr::{null, null_mut};
fn config() -> WsSliceConfig {
    let time =
        SimTime::from_utc(ws_sim::UtcDateTime::new(2026, 6, 22, 11, 22, 30, 0).unwrap()).unwrap();
    WsSliceConfig {
        abi_version: ABI_VERSION,
        struct_size: 32,
        seed: 71449,
        start_us: time.as_micros_since_epoch(),
        reserved: 0,
    }
}
fn op(tag: u32, value: u64) -> WsSliceOperation {
    WsSliceOperation {
        abi_version: ABI_VERSION,
        struct_size: 64,
        tag,
        reserved: 0,
        words: [value, 0, 0, 0, 0, 0],
    }
}
fn u32_from(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn u64_from(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

#[test]
fn slice_config_operation_layout_and_old_library_creation_refusal_are_exact() {
    assert_eq!(
        (size_of::<WsSliceConfig>(), align_of::<WsSliceConfig>()),
        (32, 8)
    );
    assert_eq!(
        (
            std::mem::offset_of!(WsSliceConfig, abi_version),
            std::mem::offset_of!(WsSliceConfig, struct_size),
            std::mem::offset_of!(WsSliceConfig, seed),
            std::mem::offset_of!(WsSliceConfig, start_us),
            std::mem::offset_of!(WsSliceConfig, reserved)
        ),
        (0, 4, 8, 16, 24)
    );
    assert_eq!(
        (
            size_of::<WsSliceOperation>(),
            align_of::<WsSliceOperation>()
        ),
        (64, 8)
    );
    assert_eq!(
        (
            std::mem::offset_of!(WsSliceOperation, abi_version),
            std::mem::offset_of!(WsSliceOperation, struct_size),
            std::mem::offset_of!(WsSliceOperation, tag),
            std::mem::offset_of!(WsSliceOperation, reserved),
            std::mem::offset_of!(WsSliceOperation, words)
        ),
        (0, 4, 8, 12, 16)
    );
    // SAFETY: only valid stack configs/null pointers; successful ownership destroyed once.
    unsafe {
        assert!(ws_slice_create(null()).is_null());
        for version in 1..=8 {
            assert!(
                ws_slice_create(&WsSliceConfig {
                    abi_version: version,
                    ..config()
                })
                .is_null()
            );
        }
        for c in [
            WsSliceConfig {
                struct_size: 0,
                ..config()
            },
            WsSliceConfig {
                reserved: 1,
                ..config()
            },
            WsSliceConfig {
                start_us: 1,
                ..config()
            },
            WsSliceConfig {
                start_us: i64::MAX,
                ..config()
            },
        ] {
            assert!(ws_slice_create(&c).is_null());
        }
        assert!(ws_slice_last_error(null_mut(), null_mut(), 0) > 1);
        let h = ws_slice_create(&config());
        assert!(!h.is_null());
        ws_slice_destroy(h);
        ws_slice_destroy(null_mut());
    }
}
#[test]
fn static_dynamic_and_header_only_sky_share_a_pure_committed_boundary() {
    // SAFETY: one live exclusive handle; all pointer buffers aligned, valid and disjoint.
    unsafe {
        let h = ws_slice_create(&config());
        assert!(!h.is_null());
        let before = ((*h).world.checkpoint(), (*h).world.operation_digest());
        let mut required = 0;
        assert_eq!(
            ws_slice_fill_geometry(h, null_mut(), 0, &mut required),
            BUFFER_TOO_SMALL
        );
        assert_eq!(required, 71_532);
        let mut geometry = vec![0x55; required];
        let mut written = 0;
        assert_eq!(
            ws_slice_fill_geometry(h, geometry.as_mut_ptr(), geometry.len(), &mut written),
            OK
        );
        assert_eq!(written, required);
        assert_eq!(
            (
                u32_from(&geometry, 0),
                u32_from(&geometry, 4),
                u64_from(&geometry, 8)
            ),
            (10, 96, ws_sim::SITE_DATASET_VERSION)
        );
        assert_eq!(
            (
                u32_from(&geometry, 20),
                u32_from(&geometry, 24),
                u32_from(&geometry, 28)
            ),
            (58, 2245, 3693)
        );
        assert_eq!(u32_from(&geometry, 44), 0);
        assert_eq!(
            (0..58)
                .filter(|i| u64_from(&geometry, 96 + i * 48 + 8) != 0)
                .count(),
            4
        );
        for _ in 0..5 {
            let mut bytes = [0x55u8; MAX_DYNAMIC];
            let mut written = 0;
            assert_eq!(
                ws_slice_fill_snapshot(h, bytes.as_mut_ptr(), bytes.len(), &mut written),
                OK
            );
            assert_eq!(written, 1760);
            assert_eq!(
                (
                    u32_from(&bytes, 0),
                    u32_from(&bytes, 4),
                    u32_from(&bytes, 48),
                    u32_from(&bytes, 52),
                    u32_from(&bytes, 56)
                ),
                (10, 128, 30, 30, 4)
            );
            assert_eq!(&bytes[92..128], &[0; 36]);
            let mut sky = std::mem::MaybeUninit::<WsCalendarSky>::uninit();
            let p = sky.as_mut_ptr();
            std::ptr::addr_of_mut!((*p).abi_version).write(ABI_VERSION);
            std::ptr::addr_of_mut!((*p).struct_size).write(160);
            assert_eq!(ws_slice_get_calendar_sky(h, p), OK);
            let sky = sky.assume_init();
            assert_eq!(
                (
                    sky.tick,
                    sky.sim_time_us as u64,
                    sky.state_hash,
                    sky.scale_tag,
                    sky.site_dataset_version
                ),
                (
                    u64_from(&bytes, 8),
                    u64_from(&bytes, 16),
                    u64_from(&bytes, 24),
                    u32_from(&bytes, 44),
                    u64_from(&bytes, 32)
                )
            );
            assert_eq!(
                ((*h).world.checkpoint(), (*h).world.operation_digest()),
                before
            );
        }
        ws_slice_destroy(h);
    }
}
#[test]
fn invalid_wire_never_enters_operations_and_short_error_outputs_remain_untouched() {
    // SAFETY: live serialized handle, explicit documented invalid/null paths and valid disjoint buffers.
    unsafe {
        let h = ws_slice_create(&config());
        let before = (
            (*h).world.checkpoint(),
            (*h).world.operation_digest(),
            (*h).world.operations().len(),
        );
        let mut invalid = vec![
            op(0, 0),
            op(99, 0),
            op(1, 101),
            op(2, 2),
            WsSliceOperation {
                abi_version: 4,
                ..op(1, 1)
            },
            WsSliceOperation {
                struct_size: 0,
                ..op(1, 1)
            },
            WsSliceOperation {
                reserved: 1,
                ..op(1, 1)
            },
        ];
        let mut extra = op(3, 0);
        extra.words[5] = 1;
        invalid.push(extra);
        let mut camera = op(4, f64::NAN.to_bits());
        camera.words[3] = 20.0f64.to_bits();
        invalid.push(camera);
        for input in invalid {
            assert_ne!(ws_slice_apply(h, &input), OK);
            assert_eq!(
                (
                    (*h).world.checkpoint(),
                    (*h).world.operation_digest(),
                    (*h).world.operations().len()
                ),
                before
            );
        }
        assert_eq!(ws_slice_apply(h, null()), INVALID_ARGUMENT);
        assert_eq!(ws_slice_apply(null_mut(), &op(1, 1)), INVALID_ARGUMENT);
        let mut bytes = [0x55u8; 16];
        let mut n = 7;
        assert_eq!(
            ws_slice_fill_snapshot(h, bytes.as_mut_ptr(), bytes.len(), &mut n),
            BUFFER_TOO_SMALL
        );
        assert_eq!(n, 1760);
        assert_eq!(bytes, [0x55; 16]);
        n = 7;
        assert_eq!(
            ws_slice_fill_snapshot(null_mut(), bytes.as_mut_ptr(), bytes.len(), &mut n),
            INVALID_ARGUMENT
        );
        assert_eq!(n, 7);
        assert_eq!(bytes, [0x55; 16]);
        n = 7;
        assert_eq!(
            ws_slice_fill_snapshot(h, null_mut(), MAX_DYNAMIC, &mut n),
            INVALID_ARGUMENT
        );
        assert_eq!(n, 7);
        assert_eq!(
            ws_slice_fill_geometry(h, bytes.as_mut_ptr(), bytes.len(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(bytes, [0x55; 16]);
        let mut sky = WsCalendarSky::from_view((*h).world.calendar_sky());
        sky.abi_version = 4;
        let before_sky = sky;
        assert_eq!(ws_slice_get_calendar_sky(h, &mut sky), VERSION_MISMATCH);
        assert_eq!(sky, before_sky);
        ws_slice_destroy(h);
    }
}
#[test]
fn coarse_camera_scale_pause_and_elapsed_use_the_existing_integer_replay_path() {
    // SAFETY: one exclusive handle and stack initialized inputs; no pointer retained.
    unsafe {
        let h = ws_slice_create(&config());
        let mut camera = op(4, 0);
        camera.words[3] = 500.0f64.to_bits();
        assert_eq!(ws_slice_apply(h, &camera), OK);
        assert!(matches!(
            (*h).world.operations().last().unwrap().operation,
            Operation::CameraInterest(_)
        ));
        assert_eq!(ws_slice_apply(h, &op(1, 20)), OK);
        assert!((*h).world.render_snapshot().unwrap().summary.l1 > 0);
        assert_eq!(ws_slice_apply(h, &op(2, 0)), OK);
        let tick = (*h).world.clock().steps();
        assert_eq!(ws_slice_apply(h, &op(3, 1_000_000)), OK);
        assert_eq!((*h).world.clock().steps(), tick);
        assert_eq!(ws_slice_apply(h, &op(2, 100)), OK);
        assert_eq!(ws_slice_apply(h, &op(3, 16_667)), OK);
        let (replayed, stopped) =
            SliceSimulation::replay((*h).world.config(), (*h).world.operations()).unwrap();
        assert!(!stopped);
        assert_eq!((*h).world.checkpoint(), replayed.checkpoint());
        assert_eq!((*h).world.operation_digest(), replayed.operation_digest());
        assert_eq!(
            (*h).world.render_snapshot().unwrap(),
            replayed.render_snapshot().unwrap()
        );
        ws_slice_destroy(h);
    }
}
#[test]
fn caught_panic_poison_preserves_query_outputs_and_only_diagnostics_and_destroy_remain() {
    // SAFETY: live exclusive handle with deliberate caught unwind, valid disjoint outputs.
    unsafe {
        let h = ws_slice_create(&config());
        assert_eq!(operate_slice(h, |_| panic!("slice poison test")), PANIC);
        let mut buffer = [0x55u8; MAX_DYNAMIC];
        let mut n = 7;
        assert_eq!(
            ws_slice_fill_snapshot(h, buffer.as_mut_ptr(), buffer.len(), &mut n),
            POISONED
        );
        assert_eq!(buffer, [0x55; MAX_DYNAMIC]);
        assert_eq!(n, 7);
        assert_eq!(
            ws_slice_fill_geometry(h, buffer.as_mut_ptr(), buffer.len(), &mut n),
            POISONED
        );
        assert_eq!(n, 7);
        let mut sky = WsCalendarSky::from_view((*h).world.calendar_sky());
        let original = sky;
        assert_eq!(ws_slice_get_calendar_sky(h, &mut sky), POISONED);
        assert_eq!(sky, original);
        assert_eq!(ws_slice_apply(h, &op(1, 1)), POISONED);
        let mut diagnostic: [std::ffi::c_char; 256] = [0; 256];
        assert!(ws_slice_last_error(h, diagnostic.as_mut_ptr(), diagnostic.len()) > 1);
        assert!(
            std::ffi::CStr::from_ptr(diagnostic.as_ptr())
                .to_str()
                .unwrap()
                .contains("poisoned")
        );
        ws_slice_destroy(h);
    }
}

fn fixture(hex: &str) -> Vec<u8> {
    let hex: String = hex.split_whitespace().collect();
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn close_f32(a: &[u8], b: &[u8], at: usize, tolerance: f32) {
    let a = f32::from_bits(u32_from(a, at));
    let b = f32::from_bits(u32_from(b, at));
    assert!((a - b).abs() <= tolerance, "offset{at}: {a} versus {b}");
}
#[test]
fn shared_real_slice_samples_pin_integers_and_respect_coordinate_determinism_tiers() {
    let expected_geometry = fixture(include_str!("../../tests/fixtures/slice-geometry-v10.hex"));
    let expected_snapshot = fixture(include_str!("../../tests/fixtures/slice-snapshot-v10.hex"));
    let expected_sky = fixture(include_str!("../../tests/fixtures/slice-sky-v10.hex"));
    // SAFETY: live exclusive native handle and initialized disjoint output storage.
    unsafe {
        let h = ws_slice_create(&config());
        let mut n = 0;
        let mut bytes = vec![0u8; expected_geometry.len()];
        assert_eq!(
            ws_slice_fill_geometry(h, bytes.as_mut_ptr(), bytes.len(), &mut n),
            OK
        );
        assert_eq!(&bytes[..48], &expected_geometry[..48]);
        for at in [48, 56, 64] {
            let a = f64::from_bits(u64_from(&bytes, at));
            let b = f64::from_bits(u64_from(&expected_geometry, at));
            assert!((a - b).abs() < 1e-9);
        }
        for at in (72..96).step_by(4) {
            close_f32(&bytes, &expected_geometry, at, 0.0001);
        }
        for i in 0..58 {
            let at = 96 + i * 48;
            assert_eq!(&bytes[at..at + 32], &expected_geometry[at..at + 32]);
            assert_eq!(
                &bytes[at + 36..at + 40],
                &expected_geometry[at + 36..at + 40]
            );
            for offset in [32, 40, 44] {
                close_f32(&bytes, &expected_geometry, at + offset, 0.0001);
            }
        }
        let vertices = 96 + 58 * 48;
        for i in 0..2245 {
            for offset in (0..24).step_by(4) {
                close_f32(
                    &bytes,
                    &expected_geometry,
                    vertices + i * 24 + offset,
                    if offset < 12 { 0.0001 } else { 0.000002 },
                );
            }
        }
        assert_eq!(
            &bytes[vertices + 2245 * 24..],
            &expected_geometry[vertices + 2245 * 24..]
        );
        let mut camera = op(4, 0);
        camera.words[3] = 500.0f64.to_bits();
        assert_eq!(ws_slice_apply(h, &camera), OK);
        assert_eq!(ws_slice_apply(h, &op(1, 20)), OK);
        let mut actual = [0u8; MAX_DYNAMIC];
        assert_eq!(
            ws_slice_fill_snapshot(h, actual.as_mut_ptr(), actual.len(), &mut n),
            OK
        );
        let actual_hash = u64_from(&actual, 24);
        assert_eq!(actual_hash, (*h).world.state_hash());
        actual[24..32].fill(0);
        assert_eq!(&actual[..128], &expected_snapshot[..128]);
        for i in 0..30 {
            let at = 128 + i * 48;
            assert_eq!(&actual[at..at + 16], &expected_snapshot[at..at + 16]);
            assert_eq!(
                &actual[at + 32..at + 48],
                &expected_snapshot[at + 32..at + 48]
            );
            for offset in [16, 20, 24, 28] {
                close_f32(&actual, &expected_snapshot, at + offset, 0.0001);
            }
        }
        assert_eq!(
            &actual[128 + 30 * 48..],
            &expected_snapshot[128 + 30 * 48..]
        );
        let mut sky = WsCalendarSky::from_view((*h).world.calendar_sky());
        assert_eq!(ws_slice_get_calendar_sky(h, &mut sky), OK);
        assert_eq!(sky.state_hash, actual_hash);
        sky.state_hash = 0;
        // Exact dedicated sun/frame bits stay Tier1; hash0 is only a transport sentinel.
        let bytes = std::slice::from_raw_parts((&sky as *const WsCalendarSky).cast::<u8>(), 160);
        assert_eq!(bytes, expected_sky);
        ws_slice_destroy(h);
    }
}
#[test]
fn slice_boundary_rejects_misaligned_requests_and_preserves_failed_sky_and_diagnostics() {
    #[repr(align(8))]
    struct Aligned([u8; 80]);
    // SAFETY: deliberately unaligned pointers are only passed to validation paths,
    // never dereferenced; all successful buffers are aligned/initialized/disjoint.
    unsafe {
        let mut memory = Aligned([0x55; 80]);
        let pointer = memory.0.as_mut_ptr().add(1);
        assert!(ws_slice_create(pointer.cast()).is_null());
        let h = ws_slice_create(&config());
        assert_eq!(ws_slice_apply(h, pointer.cast()), INVALID_ARGUMENT);
        assert_eq!(
            ws_slice_get_calendar_sky(h, pointer.cast()),
            INVALID_ARGUMENT
        );
        let mut bytes = [0x55; 16];
        assert_eq!(
            ws_slice_fill_snapshot(h, bytes.as_mut_ptr(), bytes.len(), pointer.cast()),
            INVALID_ARGUMENT
        );
        assert_eq!(memory.0, [0x55; 80]);
        let mut sky = WsCalendarSky::from_view((*h).world.calendar_sky());
        sky.struct_size = 159;
        let original = sky;
        assert_eq!(ws_slice_get_calendar_sky(h, &mut sky), INVALID_ARGUMENT);
        assert_eq!(sky, original);
        let mut n = 7;
        assert_eq!(
            ws_slice_fill_geometry(h, null_mut(), 71_532, &mut n),
            INVALID_ARGUMENT
        );
        assert_eq!(n, 7);
        (*h).error.set("éé!");
        let mut diagnostic: [std::ffi::c_char; 3] = [99; 3];
        assert_eq!(ws_slice_last_error(h, diagnostic.as_mut_ptr(), 3), 6);
        assert_eq!(
            std::ffi::CStr::from_ptr(diagnostic.as_ptr())
                .to_str()
                .unwrap(),
            "é"
        );
        assert_eq!(ws_slice_last_error(h, null_mut(), 0), 6);
        assert_eq!(ws_slice_last_error(h, null_mut(), 1), 0);
        ws_slice_destroy(h);
    }
}

#[test]
fn all_three_queries_publish_nothing_when_an_actual_prepublication_unwind_is_caught() {
    #[repr(align(8))]
    struct SkyBytes([u8; 160]);
    // SAFETY: each loop owns a fresh handle and valid disjoint initialized outputs.
    unsafe {
        for kind in 0..3 {
            let h = ws_slice_create(&config());
            (*h).panic_before_output = true;
            let mut output = vec![0x55u8; 71_532];
            let mut written = 7;
            let mut sky = SkyBytes([0x55; 160]);
            sky.0[..4].copy_from_slice(&ABI_VERSION.to_le_bytes());
            sky.0[4..8].copy_from_slice(&160u32.to_le_bytes());
            let before = sky.0;
            let status = match kind {
                0 => ws_slice_fill_geometry(h, output.as_mut_ptr(), output.len(), &mut written),
                1 => ws_slice_fill_snapshot(h, output.as_mut_ptr(), output.len(), &mut written),
                _ => ws_slice_get_calendar_sky(h, sky.0.as_mut_ptr().cast()),
            };
            assert_eq!(status, PANIC);
            assert_eq!(written, 7);
            assert!(output.iter().all(|v| *v == 0x55));
            assert_eq!(sky.0, before);
            assert!((*h).poisoned);
            ws_slice_destroy(h);
        }
    }
}

#[test]
fn inspection_is_bounded_coherent_pure_and_preserves_data_on_every_error() {
    // SAFETY: exclusive handles and disjoint initialized storage, destroyed once.
    unsafe {
        let h = ws_slice_create(&config());
        assert!(!h.is_null());
        let checkpoint = (*h).world.checkpoint();
        let operations = (*h).world.operation_digest();
        let resident = (*h).world.population().residents().ids()[0].raw();
        let building = (*h).world.population().world().buildings().ids()[0].raw();
        let mut bytes = [0xa5; 8192];
        let mut written = 777;
        for (id, kind, state_size) in [(resident, 1, 112), (building, 2, 48)] {
            assert_eq!(
                ws_slice_inspect(h, id, null_mut(), 0, &mut written),
                BUFFER_TOO_SMALL
            );
            let required = written;
            assert!((96..=8192).contains(&required));
            assert_eq!(
                ws_slice_inspect(h, id, bytes.as_mut_ptr(), required - 1, &mut written),
                BUFFER_TOO_SMALL
            );
            assert!(bytes.iter().all(|b| *b == 0xa5));
            assert_eq!(written, required);
            assert_eq!(
                ws_slice_inspect(h, id, bytes.as_mut_ptr(), bytes.len(), &mut written),
                OK
            );
            assert_eq!(written, required);
            assert_eq!(u32_from(&bytes, 0), ABI_VERSION);
            assert_eq!(u32_from(&bytes, 4), 96);
            assert_eq!(u64_from(&bytes, 8), checkpoint.tick);
            assert_eq!(u64_from(&bytes, 24), checkpoint.state);
            assert_eq!(u64_from(&bytes, 40), id);
            assert_eq!(u32_from(&bytes, 48), 3);
            assert_eq!(u32_from(&bytes, 52), kind);
            assert_eq!(u32_from(&bytes, 56) as usize, required);
            assert_eq!(u32_from(&bytes, 68), state_size);
            assert_eq!(u32_from(&bytes, 96), 2);
            if kind == 1 {
                let at = 96 + u32_from(&bytes, 64) as usize;
                let v = (*h)
                    .world
                    .vitals()
                    .get(ws_sim::EntityId::from_raw(id).unwrap())
                    .unwrap();
                assert_eq!(u32_from(&bytes, 136), v.sex() as u32);
                assert_eq!(u32_from(&bytes, at + 56), u32::from(v.health()));
                assert_eq!(u32_from(&bytes, at + 60), v.life() as u32);
            }
            assert_eq!((u32_from(&bytes, 80), u32_from(&bytes, 84)), (48, 80));
            assert!(bytes[required..].iter().all(|b| *b == 0xa5));
            bytes.fill(0xa5);
        }
        for id in [0, 1, (*h).world.population().households().ids()[0].raw()] {
            written = 777;
            assert_eq!(
                ws_slice_inspect(h, id, bytes.as_mut_ptr(), bytes.len(), &mut written),
                INVALID_ARGUMENT
            );
            assert_eq!(written, 0);
            assert!(bytes.iter().all(|b| *b == 0xa5));
        }
        written = 777;
        assert_eq!(
            ws_slice_inspect(h, resident, null_mut(), 8192, &mut written),
            INVALID_ARGUMENT
        );
        assert_eq!(written, 0);
        assert_eq!(
            ws_slice_inspect(h, resident, bytes.as_mut_ptr(), 8192, null_mut()),
            INVALID_ARGUMENT
        );
        let mut misaligned_control = [0usize; 2];
        let misaligned = misaligned_control
            .as_mut_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<usize>();
        assert_eq!(
            ws_slice_inspect(h, resident, bytes.as_mut_ptr(), 8192, misaligned),
            INVALID_ARGUMENT
        );
        assert_eq!(misaligned_control, [0; 2]);
        assert_eq!((*h).world.checkpoint(), checkpoint);
        assert_eq!((*h).world.operation_digest(), operations);
        (*h).panic_before_output = true;
        written = 777;
        assert_eq!(
            ws_slice_inspect(h, resident, bytes.as_mut_ptr(), 8192, &mut written),
            PANIC
        );
        assert_eq!(written, 0);
        assert!(bytes.iter().all(|b| *b == 0xa5));
        assert_eq!(
            ws_slice_inspect(h, resident, bytes.as_mut_ptr(), 8192, &mut written),
            POISONED
        );
        ws_slice_destroy(h);
        assert_eq!(
            ws_slice_inspect(null_mut(), resident, bytes.as_mut_ptr(), 8192, &mut written),
            INVALID_ARGUMENT
        );
    }
}

#[test]
fn illness_wire_semantics_health_and_bounded_explicit_consequences_are_coherent() {
    // SAFETY: exclusive live handle and disjoint initialized inputs/outputs.
    unsafe {
        let h = ws_slice_create(&config());
        let id = (*h).world.population().residents().ids()[0];
        let before = (*h).world.checkpoint();
        let journal = (*h).world.operations().len();
        let mut command = op(5, 0);
        command.words = [id.raw(), u64::from(u32::MAX) + 1, 100_000, 0, 0, 0];
        assert_eq!(ws_slice_apply(h, &command), INVALID_COMMAND);
        assert_eq!((*h).world.operations().len(), journal);
        command.words[1] = 0;
        assert_eq!(ws_slice_apply(h, &command), STEP_FAILED);
        assert_eq!((*h).world.operations().len(), journal + 1);
        assert_eq!((*h).world.checkpoint(), before);
        command.words[1] = 5;
        command.words[2] = 200_000;
        assert_eq!(ws_slice_apply(h, &command), OK);
        assert_eq!((*h).world.vitals().get(id).unwrap().health(), 55);
        let root = (*h).world.illness_state(id).unwrap().own_root;
        let mut out = [0xa5u8; 8192];
        let mut written = 999;
        assert_eq!(
            ws_slice_consequences(h, root, null_mut(), 0, &mut written),
            BUFFER_TOO_SMALL
        );
        let required = written;
        assert!((352..=2144).contains(&required));
        assert_eq!(
            ws_slice_consequences(h, root, out.as_mut_ptr(), required - 1, &mut written),
            BUFFER_TOO_SMALL
        );
        assert!(out.iter().all(|b| *b == 0xa5));
        assert_eq!(written, required);
        let checkpoint = (*h).world.checkpoint();
        let operations = (*h).world.operation_digest();
        assert_eq!(
            ws_slice_consequences(h, root, out.as_mut_ptr(), 8192, &mut written),
            OK
        );
        assert_eq!(u32_from(&out, 0), 10);
        assert_eq!(u32_from(&out, 4), 96);
        assert_eq!(u64_from(&out, 24), checkpoint.state);
        assert_eq!(u64_from(&out, 40), root);
        assert_eq!(u64_from(&out, 48), id.raw());
        assert_eq!(u32_from(&out, 64), 128);
        assert_eq!(u32_from(&out, 96 + 32), 13);
        assert_eq!(u32_from(&out, 96 + 128 + 32), 5);
        assert_eq!(u64_from(&out, 96 + 128 + 88), root);
        assert_eq!(u32_from(&out, 96 + 128 + 96), 0);
        assert_eq!(u32_from(&out, 96 + 128 + 100), 0);
        assert!(out[written..].iter().all(|b| *b == 0xa5));
        assert_eq!((*h).world.checkpoint(), checkpoint);
        assert_eq!((*h).world.operation_digest(), operations);
        out.fill(0xa5);
        for unknown in [0, 1, u64::MAX] {
            written = 999;
            assert_eq!(
                ws_slice_consequences(h, unknown, out.as_mut_ptr(), 8192, &mut written),
                INVALID_ARGUMENT
            );
            assert_eq!(written, 0);
            assert!(out.iter().all(|b| *b == 0xa5));
        }
        assert_eq!(
            ws_slice_consequences(h, root, out.as_mut_ptr(), 8192, null_mut()),
            INVALID_ARGUMENT
        );
        let mut steps = op(1, 0);
        steps.words[0] = 2;
        assert_eq!(ws_slice_apply(h, &steps), OK);
        assert_eq!((*h).world.vitals().get(id).unwrap().health(), 100);
        assert_eq!(
            ws_slice_consequences(h, root, out.as_mut_ptr(), 8192, &mut written),
            OK
        );
        let count = u32_from(&out, 60) as usize;
        assert_eq!(u32_from(&out, 96 + (count - 1) * 128 + 32), 12);
        ws_slice_destroy(h);
    }
}
#[test]
fn consequence_unwind_poison_and_null_outputs_preserve_all_bytes() {
    // SAFETY: exclusive live handles; initialized disjoint output/control storage.
    unsafe {
        let h = ws_slice_create(&config());
        let id = (*h).world.population().residents().ids()[0];
        let mut command = op(5, 0);
        command.words = [id.raw(), 5, 100_000, 0, 0, 0];
        assert_eq!(ws_slice_apply(h, &command), OK);
        let root = (*h).world.illness_state(id).unwrap().own_root;
        let mut out = [0xa5u8; 8192];
        let mut written = 99;
        assert_eq!(
            ws_slice_consequences(h, root, null_mut(), 8192, &mut written),
            INVALID_ARGUMENT
        );
        assert_eq!(written, 0);
        let mut control = [0usize; 2];
        let bad = control.as_mut_ptr().cast::<u8>().add(1).cast::<usize>();
        assert_eq!(
            ws_slice_consequences(h, root, out.as_mut_ptr(), 8192, bad),
            INVALID_ARGUMENT
        );
        (*h).panic_before_output = true;
        assert_eq!(
            ws_slice_consequences(h, root, out.as_mut_ptr(), 8192, &mut written),
            PANIC
        );
        assert_eq!(written, 0);
        assert!(out.iter().all(|b| *b == 0xa5));
        assert_eq!(
            ws_slice_consequences(h, root, out.as_mut_ptr(), 8192, &mut written),
            POISONED
        );
        assert_eq!(written, 0);
        assert!(out.iter().all(|b| *b == 0xa5));
        ws_slice_destroy(h);
    }
}

#[test]
fn slice_ffi_save_and_load_round_trip() {
    unsafe {
        let dir = std::env::temp_dir().join(format!("ws-ffi-slice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("save.wssave");
        let path_c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();

        let h = ws_slice_create(&config());
        assert!(!h.is_null());

        // Step 15 ticks
        assert_eq!(ws_slice_apply(h, &op(1, 15)), OK);

        // Afflict resident
        let id = (*h).world.population().residents().ids()[0];
        let mut command = op(5, 0);
        command.words = [id.raw(), 4, 15_000_000, 0, 0, 0];
        assert_eq!(ws_slice_apply(h, &command), OK);

        // Step 15 more ticks
        assert_eq!(ws_slice_apply(h, &op(1, 15)), OK);

        let orig_hash = (*h).world.state_hash();

        // Null argument checks
        assert_eq!(ws_slice_save(null_mut(), path_c.as_ptr()), INVALID_ARGUMENT);
        assert_eq!(ws_slice_save(h, null()), INVALID_ARGUMENT);

        // Synchronous save
        assert_eq!(ws_slice_save(h, path_c.as_ptr()), OK);
        assert_eq!(ws_slice_save_status(h), OK);
        assert!(path.is_file());

        // Null path load
        assert!(ws_slice_load(null()).is_null());

        // Missing file load
        let missing_c = std::ffi::CString::new("/nonexistent/file.wssave").unwrap();
        assert!(ws_slice_load(missing_c.as_ptr()).is_null());

        // Successful load
        let loaded_h = ws_slice_load(path_c.as_ptr());
        assert!(!loaded_h.is_null());
        assert_eq!((*loaded_h).world.state_hash(), orig_hash);

        // Step both handles and compare hashes
        for _ in 0..20 {
            assert_eq!(ws_slice_apply(h, &op(1, 1)), OK);
            assert_eq!(ws_slice_apply(loaded_h, &op(1, 1)), OK);
            assert_eq!((*loaded_h).world.state_hash(), (*h).world.state_hash());
        }

        ws_slice_destroy(h);
        ws_slice_destroy(loaded_h);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn slice_ffi_background_save_and_status() {
    unsafe {
        let dir = std::env::temp_dir().join(format!("ws-ffi-slice-bg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bg.wssave");
        let path_c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();

        let h = ws_slice_create(&config());
        assert!(!h.is_null());
        assert_eq!(ws_slice_apply(h, &op(1, 20)), OK);
        let orig_hash = (*h).world.state_hash();

        assert_eq!(
            ws_slice_save_background(null_mut(), path_c.as_ptr()),
            INVALID_ARGUMENT
        );
        assert_eq!(ws_slice_save_background(h, null()), INVALID_ARGUMENT);

        // Launch background save
        assert_eq!(ws_slice_save_background(h, path_c.as_ptr()), OK);

        // Poll status until OK
        loop {
            let status = ws_slice_save_status(h);
            if status == OK {
                break;
            }
            assert_eq!(status, SAVE_IN_PROGRESS);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // Verify status remains OK
        assert_eq!(ws_slice_save_status(h), OK);

        // Load and verify
        let loaded = ws_slice_load(path_c.as_ptr());
        assert!(!loaded.is_null());
        assert_eq!((*loaded).world.state_hash(), orig_hash);

        ws_slice_destroy(h);
        ws_slice_destroy(loaded);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn slice_op(tag: u32, words: [u64; 6]) -> WsSliceOperation {
    WsSliceOperation {
        abi_version: ABI_VERSION,
        struct_size: 64,
        tag,
        reserved: 0,
        words,
    }
}
fn row_tag(out: &[u8], i: usize) -> u32 {
    u32_from(out, 96 + i * 128 + 32)
}
fn row_count(out: &[u8]) -> usize {
    u32_from(out, 60) as usize
}
/// Reads one consequence record.
unsafe fn consequences_of(h: *mut WsSliceHandle, root: u64) -> Vec<u8> {
    let mut out = vec![0u8; 8192];
    let mut written = 0;
    // SAFETY: caller passes a live exclusive handle; output and control are disjoint.
    assert_eq!(
        unsafe { ws_slice_consequences(h, root, out.as_mut_ptr(), out.len(), &mut written) },
        OK
    );
    out.truncate(written);
    out
}
fn last_root(h: &WsSliceHandle) -> u64 {
    h.world
        .events()
        .records()
        .iter()
        .rev()
        .find(|e| e.record.payload.is_slice_root())
        .unwrap()
        .id
        .0
}
fn featured_building(h: &WsSliceHandle, role: &str) -> ws_sim::EntityId {
    h.world
        .population()
        .world()
        .buildings()
        .iter()
        .find(|(_, b)| b.featured == Some(role))
        .unwrap()
        .0
}
/// Monday 21:00 model time (the ws-sim `gather_world` start), with a 1 km
/// camera over the block so every resident is at L1 and a gathering finds
/// attendees. Geometry is the shared block geometry, identical for every
/// slice world.
unsafe fn gather_handle() -> *mut WsSliceHandle {
    // SAFETY: a valid stack config and inputs; the returned handle is live and exclusive.
    unsafe {
        let h = ws_slice_create(&config());
        assert!(!h.is_null());
        (*h).world = SliceSimulation::new(SliceConfig {
            seed: 42,
            start: SimTime::from_micros_since_epoch(21 * 3_600_000_000),
            schedule_epoch: SimTime::EPOCH,
        })
        .unwrap();
        let mut camera = op(4, 0);
        camera.words[3] = 1000.0f64.to_bits();
        assert_eq!(ws_slice_apply(h, &camera), OK);
        for _ in 0..100 {
            let summary = (*h).world.render_snapshot().unwrap().summary;
            if summary.l1 == summary.population {
                return h;
            }
            assert_eq!(ws_slice_apply(h, &op(1, 1)), OK);
        }
        panic!("camera never promoted the block to L1");
    }
}

#[test]
fn abi8_config_is_refused() {
    assert_eq!(ABI_VERSION, 10);
    // SAFETY: valid stack configs and inputs; a successful handle is destroyed once.
    unsafe {
        assert_eq!(ws_abi_version(), 10);
        assert!(
            ws_slice_create(&WsSliceConfig {
                abi_version: 8,
                ..config()
            })
            .is_null()
        );
        assert!(
            ws_create(&WsConfig {
                abi_version: 8,
                struct_size: 32,
                seed: 7,
                start_us: 0,
                residents_per_building: 1,
                reserved: 0,
            })
            .is_null()
        );
        let h = ws_slice_create(&config());
        assert!(!h.is_null());
        let journal = (*h).world.operations().len();
        let mut old = op(1, 1);
        old.abi_version = 8;
        assert_eq!(ws_slice_apply(h, &old), VERSION_MISMATCH);
        assert_eq!((*h).world.operations().len(), journal);
        let mut sky = WsCalendarSky::from_view((*h).world.calendar_sky());
        sky.abi_version = 8;
        assert_eq!(ws_slice_get_calendar_sky(h, &mut sky), VERSION_MISMATCH);
        ws_slice_destroy(h);
    }
}

#[test]
fn abi9_config_is_refused() {
    assert_eq!(ABI_VERSION, 10);
    // SAFETY: valid stack configs and inputs; a successful handle is destroyed once.
    unsafe {
        assert_eq!(ws_abi_version(), 10);
        assert!(
            ws_slice_create(&WsSliceConfig {
                abi_version: 9,
                ..config()
            })
            .is_null()
        );
        assert!(
            ws_create(&WsConfig {
                abi_version: 9,
                struct_size: 32,
                seed: 7,
                start_us: 0,
                residents_per_building: 1,
                reserved: 0,
            })
            .is_null()
        );
        let h = ws_slice_create(&config());
        assert!(!h.is_null());
        let journal = (*h).world.operations().len();
        let mut old = op(1, 1);
        old.abi_version = 9;
        assert_eq!(ws_slice_apply(h, &old), VERSION_MISMATCH);
        assert_eq!((*h).world.operations().len(), journal);
        let mut sky = WsCalendarSky::from_view((*h).world.calendar_sky());
        sky.abi_version = 9;
        assert_eq!(ws_slice_get_calendar_sky(h, &mut sky), VERSION_MISMATCH);
        ws_slice_destroy(h);
    }
}

fn god_of(h: *mut WsSliceHandle) -> WsSliceGod {
    // SAFETY: all-zero is a valid WsSliceGod; struct_size is set before the call.
    unsafe {
        let mut out: WsSliceGod = std::mem::zeroed();
        out.struct_size = 560;
        assert_eq!(ws_slice_god(h, &mut out), OK);
        out
    }
}

#[test]
fn god_struct_reports_goals_faith_and_zeroes_unused() {
    assert_eq!(
        (size_of::<WsSliceGod>(), align_of::<WsSliceGod>()),
        (560, 8)
    );
    assert_eq!(
        (
            std::mem::offset_of!(WsSliceGod, struct_size),
            std::mem::offset_of!(WsSliceGod, reserved),
            std::mem::offset_of!(WsSliceGod, day),
            std::mem::offset_of!(WsSliceGod, block_faith),
            std::mem::offset_of!(WsSliceGod, goal_count),
            std::mem::offset_of!(WsSliceGod, goals),
            std::mem::offset_of!(WsSliceGod, resident_count),
            std::mem::offset_of!(WsSliceGod, reserved2),
            std::mem::offset_of!(WsSliceGod, residents)
        ),
        (0, 4, 8, 16, 20, 24, 72, 76, 80)
    );
    assert_eq!(
        (size_of::<WsSliceGoal>(), size_of::<WsSliceFaith>()),
        (16, 16)
    );
    // SAFETY: live exclusive handle and initialized outputs; destroyed once.
    unsafe {
        let h = ws_slice_create(&config());
        let ids = (*h).world.population().residents().ids().to_vec();
        let fresh = god_of(h);
        assert_eq!(fresh.block_faith, 0);
        assert_eq!(fresh.resident_count as usize, ids.len());
        for tag_resident in [ids[0], ids[1]] {
            assert_eq!(
                ws_slice_apply(h, &slice_op(6, [tag_resident.raw(), 0, 0, 0, 0, 0])),
                OK
            );
        }
        let out = god_of(h);
        let god = (*h).world.god();
        assert_eq!((out.struct_size, out.reserved, out.reserved2), (560, 0, 0));
        assert_eq!(out.day, god.goals().day);
        assert_eq!(out.block_faith, god.block_faith());
        assert_ne!(out.block_faith, 0, "two blessings move block faith");
        let goals = god.goals().goals();
        assert_eq!(out.goal_count as usize, goals.len());
        for (i, g) in goals.iter().enumerate() {
            let row = out.goals[i];
            assert_eq!(
                (row.kind, row.target, row.progress, row.done),
                (
                    u32::from(g.kind as u8),
                    u32::from(g.target),
                    u32::from(g.progress),
                    u32::from(g.done)
                )
            );
        }
        for row in &out.goals[goals.len()..] {
            assert_eq!((row.kind, row.target, row.progress, row.done), (0, 0, 0, 0));
        }
        assert_eq!(out.resident_count as usize, god.faith().len());
        for (i, (id, faith)) in god.faith().iter().enumerate() {
            assert_eq!(
                (
                    out.residents[i].id,
                    out.residents[i].faith,
                    out.residents[i].reserved
                ),
                (id.raw(), i32::from(*faith), 0)
            );
        }
        let blessed = god
            .faith()
            .iter()
            .filter(|(id, _)| *id == ids[0] || *id == ids[1])
            .all(|(_, f)| *f > 0);
        assert!(blessed, "both blessed residents gained faith");
        for row in &out.residents[god.faith().len()..] {
            assert_eq!((row.id, row.faith, row.reserved), (0, 0, 0));
        }
        // Dirty output is fully overwritten, including unused rows.
        let mut dirty: WsSliceGod = std::mem::zeroed();
        std::ptr::write_bytes((&mut dirty as *mut WsSliceGod).cast::<u8>(), 0xAB, 560);
        dirty.struct_size = 560;
        assert_eq!(ws_slice_god(h, &mut dirty), OK);
        assert_eq!(
            std::slice::from_raw_parts((&dirty as *const WsSliceGod).cast::<u8>(), 560),
            std::slice::from_raw_parts((&out as *const WsSliceGod).cast::<u8>(), 560)
        );
        ws_slice_destroy(h);
    }
}

#[test]
fn god_rejects_wrong_size_and_null() {
    // SAFETY: live exclusive handle and stack outputs; destroyed once.
    unsafe {
        let h = ws_slice_create(&config());
        let mut out: WsSliceGod = std::mem::zeroed();
        assert_eq!(ws_slice_god(null_mut(), &mut out), INVALID_ARGUMENT);
        assert_eq!(ws_slice_god(h, null_mut()), INVALID_ARGUMENT);
        let mut control = [0u64; 70];
        control[0] = 560;
        let bad = control
            .as_mut_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<WsSliceGod>();
        assert_eq!(ws_slice_god(h, bad), INVALID_ARGUMENT);
        for size in [0, 559, 561, 16] {
            let mut sized: WsSliceGod = std::mem::zeroed();
            sized.struct_size = size;
            sized.day = 77;
            assert_eq!(ws_slice_god(h, &mut sized), VERSION_MISMATCH);
            assert_eq!((sized.day, sized.struct_size), (77, size));
        }
        out.struct_size = 560;
        assert_eq!(operate_slice(h, |_| panic!("god poison test")), PANIC);
        assert_eq!(ws_slice_god(h, &mut out), POISONED);
        assert_eq!(out.day, 0);
        ws_slice_destroy(h);
    }
}

#[test]
fn tags_6_to_8_decode_and_reject_nonzero_unused_words() {
    const HOUR: u64 = 3_600_000_000;
    // SAFETY: live exclusive handles and stack initialized inputs; destroyed once.
    unsafe {
        let h = ws_slice_create(&config());
        let resident = (*h).world.population().residents().ids()[0];
        let building = featured_building(&*h, "rowhouse");
        let bless = slice_op(6, [resident.raw(), 0, 0, 0, 0, 0]);
        let storm = slice_op(7, [HOUR, 0, 0, 0, 0, 0]);
        let gather = slice_op(8, [building.raw(), 10 * 60_000_000, 0, 0, 0, 0]);
        let state = |h: *mut WsSliceHandle| {
            (
                (*h).world.checkpoint(),
                (*h).world.operations().len(),
                (*h).world.events().records().len(),
            )
        };
        // Framing: every unused word must be zero, and nothing is journalled.
        for (input, first_unused) in [(bless, 1), (storm, 1), (gather, 2)] {
            for i in first_unused..6 {
                let mut bad = input;
                bad.words[i] = 1;
                let before = state(h);
                assert_eq!(ws_slice_apply(h, &bad), INVALID_COMMAND, "tag{}", input.tag);
                assert_eq!(state(h), before);
            }
        }
        // Framing: a zero identity is not an entity.
        for bad in [
            slice_op(6, [0; 6]),
            slice_op(8, [0, 600_000_000, 0, 0, 0, 0]),
        ] {
            let before = state(h);
            assert_eq!(ws_slice_apply(h, &bad), INVALID_ARGUMENT);
            assert_eq!(state(h), before);
        }
        // Semantic rejections are journalled with no state or event effect.
        for bad in [
            slice_op(6, [building.raw(), 0, 0, 0, 0, 0]),
            slice_op(7, [0, 0, 0, 0, 0, 0]),
            slice_op(7, [HOUR + 100_000, 0, 0, 0, 0, 0]),
            slice_op(8, [resident.raw(), 600_000_000, 0, 0, 0, 0]),
            slice_op(8, [building.raw(), 60_000_000, 0, 0, 0, 0]),
        ] {
            let (checkpoint, journal, events) = state(h);
            assert_eq!(ws_slice_apply(h, &bad), STEP_FAILED, "{bad:?}");
            assert_eq!(state(h), (checkpoint, journal + 1, events));
        }
        // Each tag decodes to its operation.
        assert_eq!(ws_slice_apply(h, &bless), OK);
        assert_eq!(
            (*h).world.operations().last().unwrap().operation,
            Operation::Bless { resident }
        );
        assert_eq!(ws_slice_apply(h, &storm), OK);
        assert_eq!(
            (*h).world.operations().last().unwrap().operation,
            Operation::Storm { duration_us: HOUR }
        );
        // A gathering during a storm is a journalled semantic rejection (Busy).
        let (checkpoint, journal, events) = state(h);
        assert_eq!(ws_slice_apply(h, &gather), STEP_FAILED);
        assert_eq!(state(h), (checkpoint, journal + 1, events));
        assert_eq!(
            (*h).world.operations().last().unwrap().operation,
            Operation::Gather {
                building,
                duration_us: 10 * 60_000_000
            }
        );
        ws_slice_destroy(h);
        let h = ws_slice_create(&config());
        assert_eq!(ws_slice_apply(h, &gather), OK);
        assert!((*h).world.gathering().is_some());
        ws_slice_destroy(h);
    }
}

#[test]
fn consequence_header_per_root() {
    const MINUTE: u64 = 60_000_000;
    // SAFETY: live exclusive handles and disjoint initialized outputs; destroyed once.
    unsafe {
        // Bless, healthy: subject is the resident, severity 0, end = applied.
        let h = ws_slice_create(&config());
        let ids = (*h).world.population().residents().ids().to_vec();
        assert_eq!(
            ws_slice_apply(h, &slice_op(6, [ids[0].raw(), 0, 0, 0, 0, 0])),
            OK
        );
        let root = last_root(&*h);
        let out = consequences_of(h, root);
        assert_eq!(u32_from(&out, 0), ABI_VERSION);
        assert_eq!(u64_from(&out, 40), root);
        assert_eq!(u64_from(&out, 48), ids[0].raw());
        assert_eq!(u32_from(&out, 76), 0);
        assert_eq!(u64_from(&out, 88), u64_from(&out, 80));
        assert_eq!(
            u64_from(&out, 80),
            (*h).world.clock().now().as_micros_since_epoch() as u64
        );
        assert_eq!(
            (row_count(&out), row_tag(&out, 0), row_tag(&out, 1)),
            (2, 14, 15)
        );
        // Bless, ill: the cure row is a Recovered (12), still severity 0, end = applied.
        let mut afflict = op(5, 0);
        afflict.words = [ids[1].raw(), 5, 600_000_000, 0, 0, 0];
        assert_eq!(ws_slice_apply(h, &afflict), OK);
        assert_eq!(ws_slice_apply(h, &op(1, 3)), OK);
        assert_eq!(
            ws_slice_apply(h, &slice_op(6, [ids[1].raw(), 0, 0, 0, 0, 0])),
            OK
        );
        let root = last_root(&*h);
        let out = consequences_of(h, root);
        assert_eq!(u64_from(&out, 48), ids[1].raw());
        assert_eq!(u32_from(&out, 76), 0);
        assert_eq!(u64_from(&out, 88), u64_from(&out, 80));
        assert_eq!(
            (row_count(&out), row_tag(&out, 0), row_tag(&out, 1)),
            (2, 14, 12)
        );
        ws_slice_destroy(h);

        // Storm: subject 0, severity 0, end = storm end; rows 16, then 18 and 19.
        let h = ws_slice_create(&config());
        assert_eq!(ws_slice_apply(h, &slice_op(7, [MINUTE, 0, 0, 0, 0, 0])), OK);
        let root = last_root(&*h);
        let storm_end = (*h).world.storm().unwrap().end;
        let out = consequences_of(h, root);
        assert_eq!(u64_from(&out, 48), 0);
        assert_eq!(u32_from(&out, 76), 0);
        assert_eq!(u64_from(&out, 88), storm_end.as_micros_since_epoch() as u64);
        assert_eq!(u64_from(&out, 88) - u64_from(&out, 80), MINUTE);
        assert_eq!((row_count(&out), row_tag(&out, 0)), (1, 16));
        assert_eq!((*h).world.apply(Operation::Steps(601)).unwrap().error, None);
        let out = consequences_of(h, root);
        assert_eq!(u64_from(&out, 48), 0);
        assert_eq!(
            (0..row_count(&out))
                .map(|i| row_tag(&out, i))
                .collect::<Vec<_>>(),
            [16, 18, 19]
        );
        // The wire subject 0 is accepted only for a storm root.
        let (group, _) = (*h)
            .world
            .consequences(ws_sim::ws_events::EventId(root))
            .unwrap();
        let mut forged = *group;
        assert_eq!(inspection::header_subject(&forged), Ok(0));
        forged.tag = 14;
        assert!(inspection::header_subject(&forged).is_err());
        forged.tag = 16;
        forged.subject = Some((*h).world.population().residents().ids()[0]);
        assert!(inspection::header_subject(&forged).is_err());
        ws_slice_destroy(h);

        // Gathering: subject is the building, severity 0, end = gathering end.
        let h = gather_handle();
        let building = featured_building(&*h, "rowhouse");
        assert_eq!(
            ws_slice_apply(h, &slice_op(8, [building.raw(), 10 * MINUTE, 0, 0, 0, 0])),
            OK
        );
        let root = last_root(&*h);
        let end = (*h).world.gathering().unwrap().end;
        let out = consequences_of(h, root);
        assert_eq!(u64_from(&out, 48), building.raw());
        assert_eq!(u32_from(&out, 76), 0);
        assert_eq!(u64_from(&out, 88), end.as_micros_since_epoch() as u64);
        assert_eq!(u64_from(&out, 88) - u64_from(&out, 80), 10 * MINUTE);
        assert_eq!((row_count(&out), row_tag(&out, 0)), (1, 20));
        ws_slice_destroy(h);
    }
}

#[test]
fn inspection_accepts_kind3_away_from_home_only() {
    use ws_sim::slice::inspect::{Entity, Location};
    // SAFETY: one live exclusive handle and disjoint initialized outputs; destroyed once.
    unsafe {
        let h = gather_handle();
        let target = featured_building(&*h, "rowhouse");
        assert_eq!(
            ws_slice_apply(h, &slice_op(8, [target.raw(), 10 * 60_000_000, 0, 0, 0, 0])),
            OK
        );
        let root = last_root(&*h);
        let attendees: Vec<_> = (*h)
            .world
            .gathering()
            .unwrap()
            .attendees()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        let away = |h: *mut WsSliceHandle| {
            attendees.iter().copied().find(|id| {
                let record = (*h).world.inspect(*id).unwrap();
                let Entity::Resident {
                    identity, state, ..
                } = &record.entity
                else {
                    unreachable!()
                };
                identity.home != target && state.logical.location == Location::AtBuilding(target)
            })
        };
        let mut guest = None;
        for _ in 0..6000 {
            guest = away(h);
            if guest.is_some() {
                break;
            }
            assert_eq!((*h).world.apply(Operation::Steps(1)).unwrap().error, None);
        }
        let guest = guest.expect("an attendee from another building reaches the target");
        // The native record publishes kind 3 at the target, away from home.
        let mut out = [0u8; 8192];
        let mut written = 0;
        assert_eq!(
            ws_slice_inspect(h, guest.raw(), out.as_mut_ptr(), out.len(), &mut written),
            OK
        );
        let state_at = 96 + u32_from(&out, 64) as usize;
        assert_eq!(u32_from(&out, state_at + 4), 5); // Home activity
        assert_eq!(u32_from(&out, state_at + 8), 3); // Socialize
        assert_eq!(u32_from(&out, state_at + 12), 0); // at a building
        assert_eq!(u64_from(&out, state_at + 32), target.raw());
        assert_eq!(u32_from(&out, state_at + 80), 3);
        assert_eq!(u64_from(&out, state_at + 88), root);
        // No other override kind may stand away from home.
        let gathering = (*h).world.gathering().map(|g| g.building);
        let featured = (*h).world.population().world().buildings().ids();
        let mut bytes = [0u8; inspection::MAX];
        let record = (*h).world.inspect(guest).unwrap();
        assert!(inspection::encode(&record, gathering, featured, &mut bytes).is_ok());
        for kind in [1, 2, 4] {
            let mut forged = (*h).world.inspect(guest).unwrap();
            forged.illness.as_mut().unwrap().override_kind = kind;
            assert!(
                inspection::encode(&forged, gathering, featured, &mut bytes).is_err(),
                "kind{kind}"
            );
        }
        // Kind 3 away from home is valid only at the active gathering's building.
        let record = (*h).world.inspect(guest).unwrap();
        assert!(inspection::encode(&record, None, featured, &mut bytes).is_err());
        let elsewhere = featured_building(&*h, "two-family house");
        assert!(inspection::encode(&record, Some(elsewhere), featured, &mut bytes).is_err());
        // A root-less override is incoherent.
        let mut forged = (*h).world.inspect(guest).unwrap();
        forged.illness.as_mut().unwrap().override_root = 0;
        assert!(inspection::encode(&forged, gathering, featured, &mut bytes).is_err());
        ws_slice_destroy(h);
    }
}

#[test]
fn event_words_rows_for_power_tags_match_adr_0038() {
    use ws_sim::ws_events::{Activity, Payload};
    let world = SliceSimulation::new(SliceConfig {
        seed: 71449,
        start: SimTime::EPOCH,
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap();
    let r = world.population().residents().ids()[0];
    let b = world.population().world().buildings().ids()[0];
    let (rr, br) = (r.raw(), b.raw());
    let rows = [
        (
            14,
            Payload::BlessApplied {
                resident: r,
                sequence: 3,
                affected: 1,
            },
            [rr, 3, 1, 0, 0],
        ),
        (15, Payload::NeedsRelieved { resident: r }, [rr, 0, 0, 0, 0]),
        (
            16,
            Payload::StormApplied {
                sequence: 4,
                duration_us: 60_000_000,
            },
            [4, 60_000_000, 0, 0, 0],
        ),
        (
            17,
            Payload::StayedInside {
                resident: r,
                building: b,
                activity: Activity::Work,
            },
            [rr, br, 2, 0, 0],
        ),
        (18, Payload::StormKeptInside { count: 6 }, [6, 0, 0, 0, 0]),
        (19, Payload::StormEnded { sequence: 4 }, [4, 0, 0, 0, 0]),
        (
            20,
            Payload::GatheringCalled {
                building: b,
                sequence: 5,
                duration_us: 600_000_000,
                attendees: 7,
            },
            [br, 5, 600_000_000, 7, 0],
        ),
        (
            21,
            Payload::JoinedGathering {
                resident: r,
                building: b,
            },
            [rr, br, 0, 0, 0],
        ),
        (
            22,
            Payload::GatheringEnded {
                building: b,
                attendees: 2,
            },
            [br, 2, 0, 0, 0],
        ),
    ];
    for (tag, payload, words) in rows {
        assert_eq!(payload.tag(), tag);
        assert_eq!(inspection::event_words(payload), Ok(words), "tag{tag}");
    }
}

#[test]
fn last_root_tracks_accepted_interventions_only() {
    const MINUTE: u64 = 60_000_000;
    // SAFETY: live exclusive handles, aligned initialized control storage; destroyed once.
    unsafe {
        let h = gather_handle();
        let read = |h: *mut WsSliceHandle| {
            let mut root = u64::MAX;
            assert_eq!(ws_slice_last_root(h, &mut root), OK);
            root
        };
        assert_eq!(read(h), 0);
        let resident = (*h).world.population().residents().ids()[0];
        let building = featured_building(&*h, "rowhouse");
        let before = ((*h).world.checkpoint(), (*h).world.operation_digest());
        assert_eq!(read(h), 0);
        assert_eq!(
            ((*h).world.checkpoint(), (*h).world.operation_digest()),
            before
        );
        let mut afflict = op(5, 0);
        afflict.words = [resident.raw(), 5, MINUTE, 0, 0, 0];
        assert_eq!(ws_slice_apply(h, &afflict), OK);
        let curse = read(h);
        assert_eq!(curse, (*h).world.illness_state(resident).unwrap().own_root);
        assert_eq!(
            ws_slice_apply(h, &slice_op(6, [resident.raw(), 0, 0, 0, 0, 0])),
            OK
        );
        let bless = read(h);
        assert!(bless > curse);
        assert_eq!(row_tag(&consequences_of(h, bless), 0), 14);
        assert_eq!(
            ws_slice_apply(h, &slice_op(8, [building.raw(), 10 * MINUTE, 0, 0, 0, 0])),
            OK
        );
        let gather = read(h);
        assert_eq!(gather, (*h).world.gathering().unwrap().root.0);
        // Rejected (Busy) and malformed operations leave the value unchanged.
        assert_eq!(
            ws_slice_apply(h, &slice_op(7, [MINUTE, 0, 0, 0, 0, 0])),
            STEP_FAILED
        );
        assert_eq!(
            ws_slice_apply(h, &slice_op(7, [MINUTE, 1, 0, 0, 0, 0])),
            INVALID_COMMAND
        );
        assert_eq!(
            ws_slice_apply(h, &slice_op(6, [building.raw(), 0, 0, 0, 0, 0])),
            STEP_FAILED
        );
        assert_eq!(read(h), gather);
        // Ordinary steps add no intervention root.
        for _ in 0..200 {
            if (*h).world.gathering().is_none() {
                break;
            }
            assert_eq!((*h).world.apply(Operation::Steps(100)).unwrap().error, None);
        }
        assert!((*h).world.gathering().is_none());
        assert_eq!(read(h), gather);
        assert_eq!(ws_slice_apply(h, &slice_op(7, [MINUTE, 0, 0, 0, 0, 0])), OK);
        let storm = read(h);
        assert_eq!(storm, (*h).world.storm().unwrap().root.0);
        let out = consequences_of(h, storm);
        assert_eq!((u64_from(&out, 48), row_tag(&out, 0)), (0, 16));
        // Null, misaligned and poisoned handling like the sibling exports.
        let mut root = 7u64;
        assert_eq!(ws_slice_last_root(null_mut(), &mut root), INVALID_ARGUMENT);
        assert_eq!(root, 7);
        assert_eq!(ws_slice_last_root(h, null_mut()), INVALID_ARGUMENT);
        let mut control = [0u64; 2];
        let bad = control.as_mut_ptr().cast::<u8>().add(1).cast::<u64>();
        assert_eq!(ws_slice_last_root(h, bad), INVALID_ARGUMENT);
        assert_eq!(control, [0; 2]);
        assert_eq!(operate_slice(h, |_| panic!("last root poison test")), PANIC);
        assert_eq!(ws_slice_last_root(h, &mut root), POISONED);
        assert_eq!(root, 7);
        ws_slice_destroy(h);
    }
}

#[test]
fn last_root_survives_save_and_load() {
    // SAFETY: live exclusive handles and a valid C path; each destroyed once.
    unsafe {
        let dir = std::env::temp_dir().join(format!("ws-ffi-last-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = std::ffi::CString::new(dir.join("root.wssave").to_str().unwrap()).unwrap();
        let h = ws_slice_create(&config());
        let resident = (*h).world.population().residents().ids()[0];
        assert_eq!(
            ws_slice_apply(h, &slice_op(6, [resident.raw(), 0, 0, 0, 0, 0])),
            OK
        );
        assert_eq!(ws_slice_apply(h, &op(1, 3)), OK);
        let mut root = 0;
        assert_eq!(ws_slice_last_root(h, &mut root), OK);
        assert_ne!(root, 0);
        assert_eq!(ws_slice_save(h, path.as_ptr()), OK);
        let loaded = ws_slice_load(path.as_ptr());
        assert!(!loaded.is_null());
        let mut after = 0;
        assert_eq!(ws_slice_last_root(loaded, &mut after), OK);
        assert_eq!(after, root);
        ws_slice_destroy(h);
        ws_slice_destroy(loaded);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Monday 07:30 (the ws-sim `storm_world` start), so a one-hour storm spans
/// the 08:00 departures; every resident at L1 as in `gather_handle`.
unsafe fn storm_handle() -> *mut WsSliceHandle {
    // SAFETY: a valid stack config and inputs; the returned handle is live and exclusive.
    unsafe {
        let h = ws_slice_create(&config());
        assert!(!h.is_null());
        (*h).world = SliceSimulation::new(SliceConfig {
            seed: 42,
            start: SimTime::from_micros_since_epoch(7 * 3_600_000_000 + 30 * 60_000_000),
            schedule_epoch: SimTime::EPOCH,
        })
        .unwrap();
        let mut camera = op(4, 0);
        camera.words[3] = 1000.0f64.to_bits();
        assert_eq!(ws_slice_apply(h, &camera), OK);
        h
    }
}

/// D1 as amended: every resident inspects OK on every tick through a one-hour
/// storm and its whole resume, and a hold is published as kind 4 with the
/// storm root, in both phases, exactly where the hold places the resident.
#[test]
fn storm_holds_inspect_as_kind4_through_storm_and_resume() {
    use ws_sim::slice::inspect::HoldPhase;
    // SAFETY: one live exclusive handle and disjoint initialized outputs; destroyed once.
    unsafe {
        let h = storm_handle();
        assert_eq!(
            ws_slice_apply(h, &slice_op(7, [3_600_000_000, 0, 0, 0, 0, 0])),
            OK
        );
        let root = last_root(&*h);
        let residents = (*h).world.population().residents().ids().to_vec();
        let mut out = [0u8; 8192];
        let (mut deferring, mut resuming_walk, mut resuming_stay) = (0, 0, 0);
        let mut ticks = 0;
        loop {
            for &id in &residents {
                let mut written = 0;
                assert_eq!(
                    ws_slice_inspect(h, id.raw(), out.as_mut_ptr(), out.len(), &mut written),
                    OK,
                    "tick {ticks} resident {}",
                    id.raw()
                );
                let state_at = 96 + u32_from(&out, 64) as usize;
                let kind = u32_from(&out, state_at + 80);
                let hold = (*h).world.illness_state(id).unwrap().hold;
                assert_eq!(kind == 4, hold.is_some());
                if let Some(hold) = hold {
                    assert_eq!(u64_from(&out, state_at + 88), root);
                    assert_eq!(hold.root, root);
                    match (hold.phase, u32_from(&out, state_at + 12)) {
                        (HoldPhase::Deferring, 0) => deferring += 1,
                        (HoldPhase::Deferring, l) => panic!("deferring at location {l}"),
                        (HoldPhase::Resuming, 2) => {
                            assert_eq!(u32_from(&out, state_at + 4), 1);
                            assert_eq!(u32_from(&out, state_at + 8), 5);
                            assert_eq!(u64_from(&out, state_at + 32), hold.building.raw());
                            resuming_walk += 1;
                        }
                        (HoldPhase::Resuming, _) => resuming_stay += 1,
                    }
                }
            }
            let held = (*h)
                .world
                .to_parts()
                .behavior_states
                .iter()
                .any(|s| s.storm_hold.is_some());
            if ticks > 36_000 && !held {
                break;
            }
            assert!(ticks < 6 * 36_000, "holds never rejoined");
            assert_eq!(ws_slice_apply(h, &op(1, 1)), OK);
            ticks += 1;
        }
        // Both phases are published as kind 4. In this scenario every post-storm walk
        // rejoins on arrival, so a resuming stay (kept activity at the destination)
        // is covered by the forged-record test and ws-life's own storm tests.
        assert!(
            deferring > 0 && resuming_walk > 0,
            "{deferring} {resuming_walk} {resuming_stay} after {ticks} ticks"
        );
        assert!((*h).world.storm().is_none());
        ws_slice_destroy(h);
    }
}

/// Kind 4 is coherent only exactly where the hold places the resident.
#[test]
fn inspection_rejects_forged_storm_holds() {
    use ws_sim::slice::inspect::Location;
    // SAFETY: one live exclusive handle and disjoint initialized outputs; destroyed once.
    unsafe {
        let h = storm_handle();
        assert_eq!(
            ws_slice_apply(h, &slice_op(7, [3_600_000_000, 0, 0, 0, 0, 0])),
            OK
        );
        let residents = (*h).world.population().residents().ids().to_vec();
        let held = loop {
            if let Some(id) = residents
                .iter()
                .copied()
                .find(|id| (*h).world.illness_state(*id).unwrap().hold.is_some())
            {
                break id;
            }
            assert_eq!(ws_slice_apply(h, &op(1, 10)), OK);
        };
        let gathering = (*h).world.gathering().map(|g| g.building);
        let featured = (*h).world.population().world().buildings().ids();
        let mut bytes = [0u8; inspection::MAX];
        let record = (*h).world.inspect(held).unwrap();
        assert!(inspection::encode(&record, gathering, featured, &mut bytes).is_ok());
        // Wrong root, a dropped hold, a routine kind, or a moved resident are refused.
        let mut forged = (*h).world.inspect(held).unwrap();
        forged.illness.as_mut().unwrap().override_root += 1;
        assert!(inspection::encode(&forged, gathering, featured, &mut bytes).is_err());
        let mut forged = (*h).world.inspect(held).unwrap();
        forged.illness.as_mut().unwrap().hold = None;
        assert!(inspection::encode(&forged, gathering, featured, &mut bytes).is_err());
        for kind in [0, 1, 2, 3, 5] {
            let mut forged = (*h).world.inspect(held).unwrap();
            forged.illness.as_mut().unwrap().override_kind = kind;
            assert!(
                inspection::encode(&forged, gathering, featured, &mut bytes).is_err(),
                "kind{kind}"
            );
        }
        let mut forged = (*h).world.inspect(held).unwrap();
        let ws_sim::slice::inspect::Entity::Resident {
            state, identity, ..
        } = &mut forged.entity
        else {
            unreachable!()
        };
        let elsewhere = featured
            .iter()
            .copied()
            .find(|b| Location::AtBuilding(*b) != state.logical.location && *b != identity.home)
            .unwrap();
        state.logical.location = Location::AtBuilding(elsewhere);
        assert!(inspection::encode(&forged, gathering, featured, &mut bytes).is_err());
        assert!(inspection::encode(&record, gathering, &[], &mut bytes).is_err());
        ws_slice_destroy(h);
    }
}

/// A bless cure whose rejoin is due exactly at a baseline departure from home,
/// under a storm, takes `end_illness`'s storm branch: the departure is deferred
/// at once, the hold is published as kind 4 with the storm root, and it
/// survives a save and load (ledger minors T4/T5).
#[test]
fn bless_cure_under_storm_defers_inspects_kind4_and_round_trips() {
    use ws_sim::slice::inspect::{HoldPhase, Location, Place};
    use ws_sim::ws_events::Payload;
    // SAFETY: live exclusive handles and a valid C path; each destroyed once.
    unsafe {
        let minute = |h: *mut WsSliceHandle| {
            for _ in 0..6 {
                assert_eq!(ws_slice_apply(h, &op(1, 100)), OK);
            }
        };
        let h = storm_handle();
        let residents = (*h).world.population().residents().ids().to_vec();
        for &id in &residents {
            assert_eq!(
                ws_slice_apply(h, &slice_op(5, [id.raw(), 5, 3 * 3_600_000_000, 0, 0, 0])),
                OK
            );
        }
        assert_eq!(
            ws_slice_apply(h, &slice_op(7, [3_600_000_000, 0, 0, 0, 0, 0])),
            OK
        );
        let storm = last_root(&*h);
        let epoch = (*h).world.schedules().epoch();
        let mut cured = None;
        for _ in 0..60 {
            minute(h);
            let now = (*h).world.clock().now();
            let due = residents.iter().copied().find(|&id| {
                let home = (*h).world.population().residents().get(id).unwrap().home;
                let base = (*h).world.schedules().get(id).unwrap().at(epoch, now);
                (*h).world.illness_state(id).unwrap().active
                    && matches!(base.location, Location::InTransit { from, departure, .. }
                        if from == Place::Building(home)
                            && departure == i128::from(now.as_micros_since_epoch()))
                    && (*h)
                        .world
                        .behavior()
                        .common(id, now)
                        .unwrap()
                        .logical
                        .location
                        == Location::AtBuilding(home)
            });
            if let Some(id) = due {
                assert_eq!(
                    ws_slice_apply(h, &slice_op(6, [id.raw(), 0, 0, 0, 0, 0])),
                    OK
                );
                cured = Some(id);
                break;
            }
        }
        let id = cured.expect("an ill resident's departure falls inside the storm");
        let bless = last_root(&*h);
        // The bless batch holds the cure and the deferral, which is linked to
        // the storm, not indexed under the bless.
        let stayed = (*h)
            .world
            .events()
            .records()
            .iter()
            .filter(|e| e.id.0 > bless)
            .filter(|e| matches!(e.record.payload, Payload::StayedInside { resident, .. } if resident == id))
            .count();
        assert_eq!(stayed, 1);
        let c = consequences_of(h, bless);
        assert_eq!((row_count(&c), row_tag(&c, 1)), (2, 12));
        assert!((*h).world.storm().unwrap().kept >= 1);
        let state = (*h).world.illness_state(id).unwrap();
        assert!(!state.active);
        let hold = state
            .hold
            .expect("the cure's rejoin is deferred by the storm");
        assert_eq!(
            (hold.phase, hold.root, state.override_kind),
            (HoldPhase::Deferring, storm, 4)
        );
        let inspect = |h: *mut WsSliceHandle| {
            let mut out = [0u8; 8192];
            let mut written = 0;
            assert_eq!(
                ws_slice_inspect(h, id.raw(), out.as_mut_ptr(), out.len(), &mut written),
                OK
            );
            let state_at = 96 + u32_from(&out, 64) as usize;
            (u32_from(&out, state_at + 80), u64_from(&out, state_at + 88))
        };
        assert_eq!(inspect(h), (4, storm));
        // Round trip: the hold, with its root, is saved and the loaded world
        // publishes the same record and the same future.
        let dir = std::env::temp_dir().join(format!("ws-ffi-cure-storm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = std::ffi::CString::new(dir.join("cure.wssave").to_str().unwrap()).unwrap();
        assert_eq!(ws_slice_save(h, path.as_ptr()), OK);
        let loaded = ws_slice_load(path.as_ptr());
        assert!(!loaded.is_null());
        assert_eq!((*loaded).world.illness_state(id).unwrap().hold, Some(hold));
        assert_eq!((*loaded).world.state_hash(), (*h).world.state_hash());
        assert_eq!(inspect(loaded), (4, storm));
        for _ in 0..90 {
            minute(h);
            minute(loaded);
            assert_eq!((*loaded).world.state_hash(), (*h).world.state_hash());
            let (kind, root) = inspect(h);
            assert_eq!(inspect(loaded), (kind, root));
            // A healthy neighbour may later check in on an ill one (kind 2).
            assert!(kind != 4 || root == storm, "kind {kind}");
        }
        assert!((*h).world.illness_state(id).unwrap().hold.is_none());
        // The storm's count includes the bless batch's deferral, and the storm
        // group stays [root, kept, ended]: StayedInside is never indexed.
        let all_stayed = (*h)
            .world
            .events()
            .records()
            .iter()
            .filter(|e| matches!(e.record.payload, Payload::StayedInside { .. }))
            .count() as u64;
        let c = consequences_of(h, storm);
        assert_eq!((row_count(&c), row_tag(&c, 1), row_tag(&c, 2)), (3, 18, 19));
        assert_eq!(u64_from(&c, 96 + 128 + 40), all_stayed);
        ws_slice_destroy(h);
        ws_slice_destroy(loaded);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

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
        for version in 1..=5 {
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
            (6, 96, ws_sim::SITE_DATASET_VERSION)
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
                (6, 128, 30, 30, 4)
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
        let mut diagnostic = [0i8; 256];
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
    let expected_geometry = fixture(include_str!("../../tests/fixtures/slice-geometry-v6.hex"));
    let expected_snapshot = fixture(include_str!("../../tests/fixtures/slice-snapshot-v6.hex"));
    let expected_sky = fixture(include_str!("../../tests/fixtures/slice-sky-v6.hex"));
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
        let mut diagnostic = [99i8; 3];
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
        for (id, kind, state_size) in [(resident, 1, 80), (building, 2, 48)] {
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
            assert_eq!(u32_from(&bytes, 48), 1);
            assert_eq!(u32_from(&bytes, 52), kind);
            assert_eq!(u32_from(&bytes, 56) as usize, required);
            assert_eq!(u32_from(&bytes, 68), state_size);
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

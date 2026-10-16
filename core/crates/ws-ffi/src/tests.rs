use super::*;
use std::ptr::{null, null_mut};
fn config(n: u32) -> WsConfig {
    WsConfig {
        abi_version: 1,
        struct_size: 32,
        seed: 7,
        start_us: 0,
        debug_marker_count: n,
        reserved: 0,
    }
}
#[test]
fn layout() {
    assert_eq!(std::mem::size_of::<WsConfig>(), 32);
    assert_eq!(std::mem::align_of::<WsConfig>(), 8);
    assert_eq!(std::mem::offset_of!(WsConfig, seed), 8);
    assert_eq!(std::mem::offset_of!(WsConfig, start_us), 16);
    assert_eq!(std::mem::offset_of!(WsConfig, debug_marker_count), 24);
    assert_eq!(std::mem::offset_of!(WsConfig, reserved), 28);
}
#[test]
fn invalid_arguments_and_config() {
    // SAFETY: documented null paths and live stack configurations; no dangling pointers.
    unsafe {
        assert_eq!(ws_abi_version(), 1);
        assert!(ws_create(null()).is_null());
        assert!(ws_last_error(null_mut(), null_mut(), 0) > 1);
        for bad in [
            WsConfig {
                abi_version: 2,
                ..config(1)
            },
            WsConfig {
                struct_size: 0,
                ..config(1)
            },
            WsConfig {
                reserved: 1,
                ..config(1)
            },
            WsConfig {
                start_us: 1,
                ..config(1)
            },
            config(100_001),
        ] {
            assert!(ws_create(&bad).is_null());
        }
        assert_eq!(ws_advance(null_mut(), 1), INVALID_ARGUMENT);
        assert_eq!(ws_submit(null_mut(), null(), 0), INVALID_ARGUMENT);
        let mut n = 9;
        assert_eq!(
            ws_fill_render_snapshot(null_mut(), null_mut(), 0, &mut n),
            INVALID_ARGUMENT
        );
        assert_eq!(n, 0);
        ws_destroy(null_mut());
    }
}
#[test]
fn snapshot_and_noop_leave_canonical_state_unchanged() {
    // SAFETY: one owned handle and disjoint stack/input/output storage; destroyed exactly once.
    unsafe {
        let h = ws_create(&config(2));
        assert!(!h.is_null());
        let hash = (*h).world.state_hash();
        let noop = [
            1u32.to_le_bytes(),
            1u32.to_le_bytes(),
            0u32.to_le_bytes(),
            0u32.to_le_bytes(),
        ]
        .concat();
        assert_eq!(ws_submit(h, noop.as_ptr(), noop.len()), OK);
        for bad in [
            vec![],
            vec![0; 7],
            [1u32.to_le_bytes(), 1025u32.to_le_bytes()].concat(),
            [
                1u32.to_le_bytes(),
                1u32.to_le_bytes(),
                1u32.to_le_bytes(),
                0u32.to_le_bytes(),
            ]
            .concat(),
            [noop.clone(), vec![0]].concat(),
        ] {
            assert_eq!(ws_submit(h, bad.as_ptr(), bad.len()), INVALID_COMMAND);
        }
        assert_eq!(ws_submit(h, null(), 9), INVALID_ARGUMENT);
        assert_eq!(ws_submit(h, null(), 65_537), INVALID_ARGUMENT);
        let mut future = noop.clone();
        future[0] = 2;
        assert_eq!(
            ws_submit(h, future.as_ptr(), future.len()),
            VERSION_MISMATCH
        );
        let empty = [1, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(ws_submit(h, empty.as_ptr(), empty.len()), OK);
        assert_eq!(ws_submit(h, null(), 0), INVALID_COMMAND);
        assert_eq!(
            validate_commands(&[2, 0, 0, 0, 0, 0, 0, 0]),
            Err((VERSION_MISMATCH, "command version mismatch"))
        );
        let mut n = 9;
        assert_eq!(
            ws_fill_render_snapshot(h, null_mut(), 0, &mut n),
            BUFFER_TOO_SMALL
        );
        assert_eq!(n, 88);
        let mut buf = [0xa5; 100];
        assert_eq!(
            ws_fill_render_snapshot(h, buf.as_mut_ptr(), 87, &mut n),
            BUFFER_TOO_SMALL
        );
        assert_eq!(buf, [0xa5; 100]);
        assert_eq!(
            ws_fill_render_snapshot(h, null_mut(), 88, &mut n),
            INVALID_ARGUMENT
        );
        assert_eq!(n, 0);
        assert_eq!(
            ws_fill_render_snapshot(h, buf.as_mut_ptr(), 100, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            ws_fill_render_snapshot(h, buf.as_mut_ptr(), 100, &mut n),
            OK
        );
        assert_eq!(&buf[88..], &[0xa5; 12]);
        assert_eq!(&buf[..8], &[1, 0, 0, 0, 24, 0, 0, 0]);
        assert_eq!(&buf[16..24], &[2, 0, 0, 0, 32, 0, 0, 0]);
        assert_eq!(&buf[24..32], &1u64.to_le_bytes());
        assert_eq!(&buf[36..40], &0.14f32.to_le_bytes());
        assert_eq!((*h).world.state_hash(), hash);
        assert_eq!(ws_advance(h, 101), INVALID_ARGUMENT);
        assert_eq!(ws_advance(h, 0), OK);
        assert_eq!(ws_advance(h, 2), OK);
        assert_eq!(ws_fill_render_snapshot(h, buf.as_mut_ptr(), 88, &mut n), OK);
        assert_eq!(&buf[8..16], &2u64.to_le_bytes());
        ws_destroy(h);
    }
}
#[test]
fn completed_ticks_survive_step_failure() {
    // SAFETY: a live owned handle, destroyed once.
    unsafe {
        let start = (i64::MAX / 100_000 - 1) * 100_000;
        let h = ws_create(&WsConfig {
            start_us: start,
            ..config(0)
        });
        assert!(!h.is_null());
        assert_eq!(ws_advance(h, 2), STEP_FAILED);
        assert_eq!((*h).world.tick(), 1);
        ws_destroy(h);
    }
}
#[test]
fn panic_poison_and_utf8_diagnostics() {
    // SAFETY: live exclusively owned handle and valid disjoint output storage.
    unsafe {
        let h = ws_create(&config(0));
        assert_eq!(operate(h, |_| panic!("injected")), PANIC);
        assert_eq!(ws_advance(h, 0), POISONED);
        (*h).error.set("éé");
        let mut out = [99i8; 4];
        assert_eq!(ws_last_error(h, out.as_mut_ptr(), 4), 5);
        assert_eq!(out.map(|x| x as u8), [0xc3, 0xa9, 0, 99]);
        assert_eq!(ws_last_error(h, null_mut(), 3), 0);
        assert_eq!(ws_last_error(h, null_mut(), 0), 5);
        ws_destroy(h);
    }
}

#[test]
fn shared_managed_fixture_matches_native_bytes() {
    let hex = include_str!("../tests/fixtures/snapshot-v1.hex").trim();
    let expected: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    // SAFETY: owned handle and disjoint correctly sized destination.
    unsafe {
        let h = ws_create(&config(1));
        let mut out = vec![0; 56];
        let mut n = 0;
        assert_eq!(
            ws_fill_render_snapshot(h, out.as_mut_ptr(), out.len(), &mut n),
            OK
        );
        assert_eq!(out, expected);
        ws_destroy(h);
    }
}

#[test]
fn snapshot_structs_match_byte_offsets() {
    assert_eq!(std::mem::size_of::<WsSnapshotHeader>(), 24);
    assert_eq!(std::mem::align_of::<WsSnapshotHeader>(), 8);
    assert_eq!(std::mem::offset_of!(WsSnapshotHeader, abi_version), 0);
    assert_eq!(std::mem::offset_of!(WsSnapshotHeader, header_size), 4);
    assert_eq!(std::mem::offset_of!(WsSnapshotHeader, tick), 8);
    assert_eq!(std::mem::offset_of!(WsSnapshotHeader, instance_count), 16);
    assert_eq!(std::mem::offset_of!(WsSnapshotHeader, record_size), 20);
    assert_eq!(std::mem::size_of::<WsInstance>(), 32);
    assert_eq!(std::mem::align_of::<WsInstance>(), 8);
    assert_eq!(std::mem::offset_of!(WsInstance, id), 0);
    assert_eq!(std::mem::offset_of!(WsInstance, kind), 8);
    assert_eq!(std::mem::offset_of!(WsInstance, east), 12);
    assert_eq!(std::mem::offset_of!(WsInstance, north), 16);
    assert_eq!(std::mem::offset_of!(WsInstance, up), 20);
    assert_eq!(std::mem::offset_of!(WsInstance, heading), 24);
    assert_eq!(std::mem::offset_of!(WsInstance, reserved), 28);
}

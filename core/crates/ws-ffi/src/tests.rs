use super::*;
use std::ptr::{null, null_mut};
fn config(n: u32) -> WsConfig {
    WsConfig {
        abi_version: 8,
        struct_size: 32,
        seed: 7,
        start_us: 0,
        residents_per_building: n,
        reserved: 0,
    }
}
#[test]
fn layout() {
    assert_eq!(std::mem::size_of::<WsConfig>(), 32);
    assert_eq!(std::mem::align_of::<WsConfig>(), 8);
    assert_eq!(std::mem::offset_of!(WsConfig, seed), 8);
    assert_eq!(std::mem::offset_of!(WsConfig, start_us), 16);
    assert_eq!(std::mem::offset_of!(WsConfig, residents_per_building), 24);
    assert_eq!(std::mem::offset_of!(WsConfig, reserved), 28);
}
#[test]
fn invalid_arguments_and_config() {
    // SAFETY: documented null paths and live stack configurations; no dangling pointers.
    unsafe {
        assert_eq!(ws_abi_version(), 8);
        assert!(ws_create(null()).is_null());
        assert!(ws_last_error(null_mut(), null_mut(), 0) > 1);
        for bad in [
            WsConfig {
                abi_version: 1,
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
            config(0),
            config(2_001),
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
            2u32.to_le_bytes(),
            1u32.to_le_bytes(),
            0u32.to_le_bytes(),
            8u32.to_le_bytes(),
            1u32.to_le_bytes(),
            2u32.to_le_bytes(),
        ]
        .concat();
        assert_eq!(ws_submit(h, noop.as_ptr(), noop.len()), OK);
        for bad in [
            vec![],
            vec![0; 7],
            [2u32.to_le_bytes(), 1025u32.to_le_bytes()].concat(),
            [
                2u32.to_le_bytes(),
                1u32.to_le_bytes(),
                99u32.to_le_bytes(),
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
        future[0] = 3;
        assert_eq!(
            ws_submit(h, future.as_ptr(), future.len()),
            VERSION_MISMATCH
        );
        let empty = [2, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(ws_submit(h, empty.as_ptr(), empty.len()), OK);
        assert_eq!(ws_submit(h, null(), 0), INVALID_COMMAND);
        assert_eq!(
            ws_submit(h, [1, 0, 0, 0, 0, 0, 0, 0].as_ptr(), 8),
            VERSION_MISMATCH
        );
        let mut n = 9;
        assert_eq!(
            ws_fill_render_snapshot(h, null_mut(), 0, &mut n),
            BUFFER_TOO_SMALL
        );
        assert_eq!(n, 504);
        let mut buf = [0xa5; 520];
        assert_eq!(
            ws_fill_render_snapshot(h, buf.as_mut_ptr(), 503, &mut n),
            BUFFER_TOO_SMALL
        );
        assert_eq!(buf, [0xa5; 520]);
        assert_eq!(
            ws_fill_render_snapshot(h, null_mut(), 504, &mut n),
            INVALID_ARGUMENT
        );
        assert_eq!(n, 0);
        assert_eq!(
            ws_fill_render_snapshot(h, buf.as_mut_ptr(), 520, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            ws_fill_render_snapshot(h, buf.as_mut_ptr(), 520, &mut n),
            OK
        );
        assert_eq!(&buf[504..], &[0xa5; 16]);
        assert_eq!(&buf[..8], &[8, 0, 0, 0, 24, 0, 0, 0]);
        assert_eq!(&buf[16..24], &[15, 0, 0, 0, 32, 0, 0, 0]);
        assert_eq!(&buf[32..36], &1u32.to_le_bytes()); // first record is a building
        assert_eq!(&buf[24 + 5 * 32 + 8..24 + 5 * 32 + 12], &2u32.to_le_bytes()); // then residents
        assert_eq!((*h).world.state_hash(), hash);
        assert_eq!(ws_advance(h, 101), INVALID_ARGUMENT);
        assert_eq!(ws_advance(h, 0), OK);
        assert_eq!((*h).world.state_hash(), hash);
        assert_eq!(ws_advance(h, 2), OK);
        assert_eq!(
            ws_fill_render_snapshot(h, buf.as_mut_ptr(), 504, &mut n),
            OK
        );
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
            ..config(1)
        });
        assert!(!h.is_null());
        let input = batch(vec![ws_sim::commands::Action::NoOp]);
        assert_eq!(ws_submit(h, input.as_ptr(), input.len()), OK);
        assert_eq!(ws_advance(h, 2), STEP_FAILED);
        assert_eq!((*h).world.journal().len(), 1);
        assert_eq!((*h).world.pending().count(), 0);
        assert_eq!((*h).world.tick(), 1);
        let mut diagnostic: [std::ffi::c_char; 128] = [0; 128];
        ws_last_error(h, diagnostic.as_mut_ptr(), diagnostic.len());
        let bytes: Vec<_> = diagnostic
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| b.to_ne_bytes()[0])
            .collect();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("overflow"));
        ws_destroy(h);
    }
}
#[test]
fn panic_poison_and_utf8_diagnostics() {
    // SAFETY: live exclusively owned handle and valid disjoint output storage.
    unsafe {
        let h = ws_create(&config(1));
        assert_eq!(operate(h, |_| panic!("injected")), PANIC);
        assert_eq!(ws_advance(h, 0), POISONED);
        (*h).error.set("éé");
        let mut out: [std::ffi::c_char; 4] = [99; 4];
        assert_eq!(ws_last_error(h, out.as_mut_ptr(), 4), 5);
        assert_eq!(out.map(|x| x.to_ne_bytes()[0]), [0xc3, 0xa9, 0, 99]);
        assert_eq!(ws_last_error(h, null_mut(), 3), 0);
        assert_eq!(ws_last_error(h, null_mut(), 0), 5);
        ws_destroy(h);
    }
}

#[test]
fn shared_managed_fixture_matches_native_bytes() {
    let hex = include_str!("../tests/fixtures/snapshot-v8.hex").trim();
    let expected: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    // SAFETY: owned handle and disjoint correctly sized destination.
    unsafe {
        let h = ws_create(&config(1));
        let mut out = vec![0; 24 + 32 * 10];
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
    assert_eq!(std::mem::offset_of!(WsInstance, a), 24);
    assert_eq!(std::mem::offset_of!(WsInstance, b), 28);
}

#[test]
fn snapshot_output_errors_use_the_live_handle_diagnostic() {
    // SAFETY: live exclusively owned handle, deliberate supported null-output
    // error paths, and disjoint writable diagnostic/output storage.
    unsafe {
        let h = ws_create(&config(1));
        assert!(!h.is_null());
        assert_eq!(ws_advance(h, 101), INVALID_ARGUMENT);
        thread_error("thread sentinel");
        assert_eq!(
            ws_fill_render_snapshot(h, null_mut(), 0, null_mut()),
            INVALID_ARGUMENT
        );
        let mut error: [std::ffi::c_char; 256] = [0; 256];
        assert_eq!(ws_last_error(h, error.as_mut_ptr(), error.len()), 16);
        assert_eq!(
            std::ffi::CStr::from_ptr(error.as_ptr()).to_str().unwrap(),
            "written is null"
        );
        // A usable handle must not route its error to the thread-local slot.
        ws_last_error(null_mut(), error.as_mut_ptr(), error.len());
        assert_eq!(
            std::ffi::CStr::from_ptr(error.as_ptr()).to_str().unwrap(),
            "thread sentinel"
        );
        // No usable handle routes the failure to the current thread instead.
        let mut written = 99;
        assert_eq!(
            ws_fill_render_snapshot(null_mut(), null_mut(), 0, &mut written),
            INVALID_ARGUMENT
        );
        assert_eq!(written, 0);
        ws_last_error(null_mut(), error.as_mut_ptr(), error.len());
        assert_eq!(
            std::ffi::CStr::from_ptr(error.as_ptr()).to_str().unwrap(),
            "handle is null"
        );
        ws_destroy(h);
    }
}

fn batch(actions: Vec<ws_sim::commands::Action>) -> Vec<u8> {
    use ws_sim::commands::{Command, IssuedBy, encode_batch};
    encode_batch(
        &actions
            .into_iter()
            .map(|a| Command::new(IssuedBy::Test, a))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn control_request() -> WsControlState {
    WsControlState {
        abi_version: 8,
        struct_size: 16,
        scale_tag: 999,
        reserved: 999,
    }
}
#[test]
fn v2_noop_is_owned_pending_then_journaled_at_zero_step_flush() {
    // SAFETY: live exclusive handle and disjoint initialized buffers.
    unsafe {
        let h = ws_create(&config(1));
        let hash = (*h).world.state_hash();
        let mut input = batch(vec![ws_sim::commands::Action::NoOp]);
        assert_eq!(ws_submit(h, input.as_ptr(), input.len()), OK);
        input.fill(0); // FFI must own the typed copy after return.
        assert_eq!((*h).world.pending().count(), 1);
        assert!((*h).world.journal().is_empty());
        assert_eq!((*h).world.events().records().len(), 1);
        let mut control = control_request();
        assert_eq!(ws_get_control_state(h, &mut control), OK);
        assert!((*h).world.journal().is_empty()); // read never flushes
        assert_eq!(ws_advance(h, 0), OK);
        assert_eq!((*h).world.journal().len(), 1);
        let outcome = &(*h).world.journal()[0];
        assert_eq!(outcome.sequence, 1);
        assert_eq!(outcome.command.action, ws_sim::commands::Action::NoOp);
        assert_eq!(outcome.result, Ok(()));
        assert_eq!((*h).world.events().records().len(), 2);
        assert_eq!((*h).world.pending().count(), 0);
        assert_eq!((*h).world.state_hash(), hash);
        assert_eq!((*h).world.tick(), 0);
        assert_eq!(ws_advance(h, 0), OK);
        assert_eq!((*h).world.journal().len(), 1);
        ws_destroy(h);
    }
}
#[test]
fn v1_clients_and_batches_are_refused_without_enqueue() {
    // SAFETY: initialized configurations, valid borrowed inputs and owned handle.
    unsafe {
        assert_ne!(ws_abi_version(), 1);
        assert!(
            ws_create(&WsConfig {
                abi_version: 1,
                ..config(1)
            })
            .is_null()
        );
        let h = ws_create(&config(1));
        let before = (*h).world.checkpoint();
        let old = [1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(ws_submit(h, old.as_ptr(), old.len()), VERSION_MISMATCH);
        assert_eq!((*h).world.checkpoint(), before);
        ws_destroy(h);
    }
}
#[test]
fn control_state_layout_validation_and_uninitialized_output_fields() {
    assert_eq!(size_of::<WsControlState>(), 16);
    assert_eq!(align_of::<WsControlState>(), 4);
    assert_eq!(std::mem::offset_of!(WsControlState, abi_version), 0);
    assert_eq!(std::mem::offset_of!(WsControlState, struct_size), 4);
    assert_eq!(std::mem::offset_of!(WsControlState, scale_tag), 8);
    assert_eq!(std::mem::offset_of!(WsControlState, reserved), 12);
    // SAFETY: exclusive handle and correctly aligned/disjoint stack storage.
    // Only the first two request fields are initialized until success writes all fields.
    unsafe {
        let h = ws_create(&config(1));
        let mut out = std::mem::MaybeUninit::<WsControlState>::uninit();
        let ptr = out.as_mut_ptr();
        std::ptr::addr_of_mut!((*ptr).abi_version).write(8);
        std::ptr::addr_of_mut!((*ptr).struct_size).write(16);
        let before = (*h).world.checkpoint();
        assert_eq!(ws_get_control_state(h, ptr), OK);
        assert_eq!(
            out.assume_init(),
            WsControlState {
                abi_version: 8,
                struct_size: 16,
                scale_tag: 1,
                reserved: 0
            }
        );
        assert_eq!((*h).world.checkpoint(), before);
        for (version, size, status) in [
            (1, 16, VERSION_MISMATCH),
            (2, 16, VERSION_MISMATCH),
            (3, 16, VERSION_MISMATCH),
            (4, 16, VERSION_MISMATCH),
            (5, 16, VERSION_MISMATCH),
            (8, 0, INVALID_ARGUMENT),
            (8, 20, INVALID_ARGUMENT),
        ] {
            let mut state = WsControlState {
                abi_version: version,
                struct_size: size,
                ..control_request()
            };
            let original = state;
            assert_eq!(ws_get_control_state(h, &mut state), status);
            assert_eq!(state, original);
        }
        let mut state = control_request();
        let original = state;
        assert_eq!(
            ws_get_control_state(null_mut(), &mut state),
            INVALID_ARGUMENT
        );
        assert_eq!(state, original);
        assert_eq!(ws_get_control_state(h, null_mut()), INVALID_ARGUMENT);
        let mut storage = [0xa5a5a5a5u32; 5];
        let unaligned = storage
            .as_mut_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<WsControlState>();
        assert_eq!(ws_get_control_state(h, unaligned), INVALID_ARGUMENT);
        assert_eq!(storage, [0xa5a5a5a5; 5]);
        assert_eq!(operate(h, |_| panic!("injected")), PANIC);
        assert_eq!(ws_get_control_state(h, &mut state), POISONED);
        assert_eq!(state, original);
        ws_destroy(h);
    }
}
#[test]
fn scale_pause_resume_explicit_steps_and_invalid_advance_before_flush() {
    use ws_sim::{TimeScale, commands::Action};
    // SAFETY: owned handle and valid disjoint command/control buffers.
    unsafe {
        let h = ws_create(&config(1));
        let pause = batch(vec![Action::SetTimeScale(TimeScale::Paused)]);
        assert_eq!(ws_submit(h, pause.as_ptr(), pause.len()), OK);
        assert_eq!(ws_advance(h, 101), INVALID_ARGUMENT);
        assert!((*h).world.journal().is_empty());
        let mut control = control_request();
        assert_eq!(ws_get_control_state(h, &mut control), OK);
        assert_eq!(control.scale_tag, 1);
        assert_eq!(ws_advance(h, 0), OK);
        assert_eq!(ws_get_control_state(h, &mut control), OK);
        assert_eq!(control.scale_tag, 0);
        assert_eq!((*h).world.tick(), 0);
        assert_eq!(ws_advance(h, 2), OK); // explicit steps ignore paused pacing
        assert_eq!((*h).world.tick(), 2);
        let resume = batch(vec![Action::SetTimeScale(TimeScale::X10)]);
        assert_eq!(ws_submit(h, resume.as_ptr(), resume.len()), OK);
        assert_eq!(ws_advance(h, 0), OK);
        assert_eq!(ws_get_control_state(h, &mut control), OK);
        assert_eq!(control.scale_tag, 10);
        assert_eq!((*h).world.journal().len(), 2);
        assert_eq!((*h).world.tick(), 2);
        ws_destroy(h);
    }
}
#[test]
fn native_ingress_caps_are_atomic_and_semantic_lod_errors_are_journaled() {
    use ws_sim::{EntityId, Lod, commands::Action};
    // SAFETY: live handle and valid borrowed byte arrays; no overlapping output.
    unsafe {
        let h = ws_create(&config(1));
        let full = batch(vec![Action::NoOp; 1024]);
        assert_eq!(ws_submit(h, full.as_ptr(), full.len()), OK);
        let before = (*h).world.checkpoint();
        let extra = batch(vec![Action::NoOp]);
        assert_eq!(ws_submit(h, extra.as_ptr(), extra.len()), INVALID_COMMAND);
        assert_eq!((*h).world.checkpoint(), before);
        assert_eq!(ws_advance(h, 0), OK);
        assert_eq!((*h).world.journal().len(), 1024);
        let id = (*h).world.toy().buildings().ids()[0];
        let input = batch(vec![
            Action::SetNodeLod(vec![(id, Lod::L2)]),
            Action::SetNodeLod(vec![(EntityId::spawned(1).unwrap(), Lod::L2)]),
        ]);
        assert_eq!(ws_submit(h, input.as_ptr(), input.len()), OK);
        assert_eq!((*h).world.journal().len(), 1024);
        assert_eq!(ws_advance(h, 1), OK);
        assert_eq!((*h).world.journal()[1024].result, Ok(()));
        assert!((*h).world.journal()[1025].result.is_err());
        assert_eq!((*h).world.tick(), 1);
        ws_destroy(h);
    }
}

#[test]
fn exact_maximum_wire_batch_and_malformed_inputs_are_atomic_at_native_boundary() {
    use ws_sim::{Lod, TimeScale, commands::Action};
    // SAFETY: valid exclusively owned handle and real byte arrays covering all supplied lengths.
    unsafe {
        let h = ws_create(&config(1));
        let id = (*h).world.toy().buildings().ids()[0];
        let mut actions = vec![Action::SetNodeLod(vec![(id, Lod::L2); 1024]); 5];
        actions.push(Action::SetTimeScale(TimeScale::X1));
        actions.extend(vec![Action::NoOp; 248]);
        let maximum = batch(actions);
        assert_eq!(maximum.len(), 65_536);
        let before = (*h).world.checkpoint();
        for bad in [
            maximum[..maximum.len() - 1].to_vec(),
            [maximum.clone(), vec![0]].concat(),
        ] {
            assert_eq!(ws_submit(h, bad.as_ptr(), bad.len()), INVALID_COMMAND);
            assert_eq!((*h).world.checkpoint(), before);
        }
        let mut schema = batch(vec![Action::NoOp]);
        schema[16] = 99;
        assert_eq!(
            ws_submit(h, schema.as_ptr(), schema.len()),
            VERSION_MISMATCH
        );
        assert_eq!((*h).world.checkpoint(), before);
        assert_eq!(ws_submit(h, maximum.as_ptr(), maximum.len()), OK);
        assert_eq!((*h).world.pending().count(), 254);
        let queued = (*h).world.checkpoint();
        let extra = batch(vec![Action::NoOp]);
        assert_eq!(ws_submit(h, extra.as_ptr(), extra.len()), INVALID_COMMAND);
        assert_eq!((*h).world.checkpoint(), queued);
        assert_eq!(ws_advance(h, 0), OK);
        assert_eq!((*h).world.journal().len(), 254);
        assert!((*h).world.journal()[..5].iter().all(|o| o.result.is_err()));
        assert!((*h).world.journal()[5..].iter().all(|o| o.result.is_ok()));
        assert_eq!((*h).world.tick(), 0);
        ws_destroy(h);
    }
}

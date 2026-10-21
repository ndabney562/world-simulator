use super::*;
use crate::{TimeScale, UtcDateTime, slice::SliceConfig};
fn world() -> SliceSimulation {
    let time = SimTime::from_utc(UtcDateTime::new(2026, 6, 22, 11, 22, 30, 0).unwrap()).unwrap();
    SliceSimulation::new(SliceConfig::calendar_aligned_v1(71449, time).unwrap()).unwrap()
}
fn identity(
    w: &SliceSimulation,
) -> (
    super::super::Checkpoint,
    u64,
    Vec<super::super::OperationRecord>,
) {
    (
        w.checkpoint(),
        w.operation_digest(),
        w.operations().to_vec(),
    )
}
#[test]
fn geometry_sky_and_bulk_queries_leave_state_history_operations_and_pending_policy_unchanged() {
    let mut w = world();
    let before = identity(&w);
    let mesh = w.render_geometry().unwrap();
    assert_eq!(mesh.buildings.len(), 58);
    for _ in 0..10 {
        let s = w.render_snapshot().unwrap();
        let sky = w.calendar_sky();
        assert_eq!(
            (s.tick, s.time, s.state_hash, s.scale),
            (sky.tick, sky.time, sky.state_hash, sky.scale_tag)
        );
        assert_eq!(s.summary.population, 30);
        assert_eq!(s.residents.len(), 30);
        assert!(s.residents.iter().all(|r| r.detail == 2
            && r.location == 0
            && r.position == [0.0; 3]
            && r.heading.to_bits() == 0));
        assert_eq!(identity(&w), before);
    }
    let op = w.camera_operation([0.0; 3], 500.0).unwrap();
    w.apply(op).unwrap();
    let before = identity(&w);
    for _ in 0..10 {
        assert_eq!(w.render_snapshot().unwrap().summary.l1, 0);
        let _ = w.calendar_sky();
        assert_eq!(identity(&w), before);
    }
}
#[test]
fn actual_fine_positions_and_all_counts_match_the_domain_across_camera_and_scale_changes() {
    let mut w = world();
    let op = w.camera_operation([0.0; 3], 500.0).unwrap();
    w.apply(op).unwrap();
    for scale in [
        TimeScale::X1,
        TimeScale::X100,
        TimeScale::Paused,
        TimeScale::X1,
    ] {
        w.apply(Operation::SetScale(scale)).unwrap();
        for _ in 0..20 {
            w.apply(Operation::Steps(1)).unwrap();
            let before = identity(&w);
            let domain = w.view().unwrap();
            let rendered = w.render_snapshot().unwrap();
            assert_eq!(rendered.summary, domain.summary);
            assert_eq!(
                rendered.summary.population,
                rendered.summary.l1 + rendered.summary.l2
            );
            assert_eq!(
                rendered.summary.population,
                rendered.summary.local_stationary
                    + rendered.summary.in_transit
                    + rendered.summary.off_map_stationary
            );
            for (r, d) in rendered.residents.iter().zip(&domain.residents) {
                assert_eq!(
                    (r.id, r.detail, r.activity),
                    (d.resident, d.detail as u32, d.logical.activity as u32)
                );
                match d.fine {
                    Some(FineLocation::Local { point, walking }) => {
                        assert_eq!(r.location, 1);
                        assert_eq!(
                            r.position,
                            w.behavior()
                                .navigation()
                                .to_render(point)
                                .unwrap()
                                .map(|v| v as f32)
                        );
                        if !walking {
                            assert_eq!(r.heading.to_bits(), 0);
                        }
                    }
                    None => assert_eq!(r.location, 0),
                    Some(FineLocation::OffMapTransit { .. }) => assert_eq!(r.location, 2),
                    Some(FineLocation::OffMap { .. }) => assert_eq!(r.location, 3),
                }
                if r.location != 1 {
                    assert_eq!(r.position, [0.0; 3]);
                    assert_eq!(r.heading.to_bits(), 0);
                }
                assert!(r.position.iter().all(|v| v.is_finite()) && r.heading.is_finite());
            }
            // The unchanged controller evaluates its cap on200-ms model boundaries.
            let policy_due = (i128::from(rendered.time.as_micros_since_epoch())
                - i128::from(w.config().schedule_epoch.as_micros_since_epoch()))
            .rem_euclid(200_000)
                == 0;
            if scale == TimeScale::X100 && policy_due {
                assert_eq!(rendered.summary.l1, 0);
            }
            assert_eq!(identity(&w), before);
        }
    }
    assert!(
        w.render_snapshot()
            .unwrap()
            .residents
            .iter()
            .any(|r| r.location == 1)
    );
}
#[test]
fn coarse_camera_normalization_is_recorded_as_integer_input_and_refuses_invalid_values() {
    let mut w = world();
    let before = identity(&w);
    for (target, radius) in [
        ([f64::NAN, 0.0, 0.0], 20.0),
        ([0.0; 3], -1.0),
        ([0.0; 3], f64::INFINITY),
        ([0.0; 3], 1_000_001.0),
        ([1_000_001.0, 0.0, 0.0], 20.0),
    ] {
        assert!(w.camera_operation(target, radius).is_err());
        assert_eq!(identity(&w), before);
    }
    let op = w.camera_operation([0.0; 3], 20.0001).unwrap();
    let Operation::CameraInterest(camera) = op else {
        panic!("expected canonical camera");
    };
    assert_eq!(camera.radius_mm, 20_001);
    assert_eq!(identity(&w), before);
    w.apply(op).unwrap();
    assert_eq!(w.operations().last().unwrap().operation, op);
    for _ in 0..12 {
        w.apply(Operation::Steps(1)).unwrap();
    }
    w.apply(Operation::SetScale(TimeScale::X100)).unwrap();
    w.apply(Operation::HostElapsed(16_667)).unwrap();
    let (replayed, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(identity(&w), identity(&replayed));
    assert_eq!(
        w.render_snapshot().unwrap(),
        replayed.render_snapshot().unwrap()
    );
    assert_eq!(w.calendar_sky(), replayed.calendar_sky());
}

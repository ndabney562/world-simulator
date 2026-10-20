//! Per-boundary environment/hash/history replay, including independent processes.
use ws_sim::{
    SimTime, TimeScale, UtcDateTime, World, WorldConfig,
    commands::{Action, Command, IssuedBy},
};
fn script() -> String {
    let start = SimTime::from_utc(UtcDateTime::new(2016, 12, 31, 23, 59, 58, 0).unwrap()).unwrap();
    let mut w = World::new(WorldConfig::new(42, start)).unwrap();
    let mut out = String::new();
    for (scale, elapsed) in [
        (TimeScale::X1, 250000),
        (TimeScale::Paused, 1000000),
        (TimeScale::X100, 100000),
        (TimeScale::X10, 10000),
        (TimeScale::Paused, 10000000),
        (TimeScale::X1, 100000),
    ] {
        w.submit(vec![Command::new(
            IssuedBy::Test,
            Action::SetTimeScale(scale),
        )])
        .unwrap();
        let pending = w.clone();
        let _ = w.calendar_sky();
        assert_eq!(w, pending, "query cannot flush pending scale");
        w.flush_commands().unwrap();
        let tick = w.tick();
        let now = w.now();
        let count = w.advance_by_host_elapsed(elapsed).unwrap();
        if scale == TimeScale::Paused {
            assert_eq!(count, 0);
            assert_eq!((w.tick(), w.now()), (tick, now));
        }
        let before = w.clone();
        let s = w.calendar_sky();
        assert_eq!(w, before);
        out.push_str(&format!("BOUNDARY {:?} {:?}\n", s, w.checkpoint()));
        // Explicit physical boundaries are independent of pacing scale and
        // exercise the distinct second60 UTC label without mutating via queries.
        for _ in 0..20 {
            w.step().unwrap();
            let before = w.clone();
            let sky = w.calendar_sky();
            assert_eq!(w, before);
            out.push_str(&format!("BOUNDARY {:?} {:?}\n", sky, w.checkpoint()));
        }
    }
    out
}
#[test]
fn calendar_speed_script_replays_every_boundary_in_same_build() {
    assert_eq!(script(), script());
}
#[test]
fn calendar_speed_script_replays_in_separate_processes() {
    const CHILD: &str = "WS_CALENDAR_REPLAY_CHILD";
    if std::env::var_os(CHILD).is_some() {
        print!("{}", script());
        return;
    }
    let run = || {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "calendar_speed_script_replays_in_separate_processes",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter(|l| l.starts_with("BOUNDARY "))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let expected: Vec<_> = script().lines().map(str::to_owned).collect();
    assert!(!expected.is_empty());
    assert_eq!(run(), expected);
    assert_eq!(run(), expected);
}

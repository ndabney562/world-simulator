use ws_sim::{SimTime, TimeScale};
use ws_sim::slice::{Operation, SliceConfig, SliceSimulation};

fn exercise() -> (u32, u64, u64, u32) {
    let config = SliceConfig { seed: 71449, start: SimTime::from_micros_since_epoch(26_550_000_000), schedule_epoch: SimTime::EPOCH };
    let mut sim = SliceSimulation::new(config).expect("real slice baseline");
    let count = sim.population().residents().len();
    assert!((10..=30).contains(&count));
    let resident = sim.population().residents().ids()[0];
    let initial = sim.state_hash();
    let result = sim.apply(Operation::Afflict { resident, severity: 5, duration_us: 5_000_000 }).expect("illness operation");
    assert!(result.error.is_none());
    assert_ne!(sim.state_hash(), initial);
    assert!(sim.apply(Operation::Steps(100)).unwrap().error.is_none());
    assert_eq!(sim.clock().steps(), 100);
    assert_eq!(sim.population().residents().len(), count);
    assert!(sim.apply(Operation::SetScale(TimeScale::Paused)).unwrap().error.is_none());
    let before = sim.state_hash();
    assert!(sim.apply(Operation::HostElapsed(1_000_000)).unwrap().error.is_none());
    assert_eq!(sim.state_hash(), before);
    let _ = sim.view().expect("coherent bulk view");
    (count as u32, sim.state_hash(), sim.clock().steps(), sim.events().records().len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn probe_population() -> u32 { exercise().0 }
#[unsafe(no_mangle)]
pub extern "C" fn probe_hash() -> u64 { exercise().1 }
#[unsafe(no_mangle)]
pub extern "C" fn probe_tick() -> u64 { exercise().2 }
#[unsafe(no_mangle)]
pub extern "C" fn probe_events() -> u32 { exercise().3 }

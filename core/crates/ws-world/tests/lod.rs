//! Simulation LOD: the L1 ↔ L2 round trip (TP-007, #8; ADR 0017 §14).
//!
//! Tier 2 tests (one build and platform): zero-time round trips are exact;
//! transitions conserve population, identity, counts, and counters; LOD
//! never changes target choices; L2 follows L1 over hours within ADR 0017's
//! tolerances; draining is cadence-independent; and insertion order,
//! request order, and cell edges change nothing.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use ws_ecs::EntityId;
use ws_math::{CellId, Ecef, Position, SECTOR_LEVEL, SurfacePos};
use ws_time::{Rate, SimDuration, SimTime};
use ws_world::ids::{Population, resident_id};
use ws_world::{
    Activity, Background, BuildingSite, Dataset, Lod, LodError, Origin, Resident, ToyWorld,
    TransitionReport, Walk, WalkParams,
};

const STEP: Rate = match Rate::new(SimDuration::from_micros(100_000)) {
    Ok(r) => r,
    Err(_) => unreachable!(),
};

/// Ticks per simulated hour at the 100 ms base step.
const HOUR: i64 = 36_000;

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        rng_seed: RngSeed::Fixed(0x5753_5f4c_4f44_0007),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// A toy world and its current boundary.
#[derive(Clone)]
struct Sim {
    w: ToyWorld,
    tick: i64,
}

impl Sim {
    fn new(w: ToyWorld) -> Self {
        Self { w, tick: 0 }
    }

    fn toy(seed: u64) -> Self {
        Self::new(toy_with(seed, Population::TOY, WalkParams::DEFAULT))
    }

    fn now(&self) -> SimTime {
        t(self.tick)
    }

    fn tick(&mut self) {
        self.w.tick(self.now(), STEP).unwrap();
        self.tick += 1;
    }

    fn run(&mut self, ticks: i64) {
        for _ in 0..ticks {
            self.tick();
        }
    }

    fn set(&mut self, req: &[(EntityId, Lod)]) -> TransitionReport {
        let r = self.w.set_lod(self.now(), STEP, req).unwrap();
        self.w.check_lod_invariants(self.now(), STEP, true).unwrap();
        r
    }

    fn all(&mut self, level: Lod) -> TransitionReport {
        let req = all(&self.w, level);
        self.set(&req)
    }

    fn check(&self) {
        self.w
            .check_lod_invariants(self.now(), STEP, false)
            .unwrap();
    }
}

fn t(ticks: i64) -> SimTime {
    SimTime::EPOCH
        .checked_add(SimDuration::from_micros(ticks * 100_000))
        .unwrap()
}

fn toy_with(seed: u64, population: Population, walk: WalkParams) -> ToyWorld {
    ToyWorld::generate(seed, &Dataset::toy().unwrap(), population, walk).unwrap()
}

fn all(w: &ToyWorld, level: Lod) -> Vec<(EntityId, Lod)> {
    w.buildings().ids().iter().map(|b| (*b, level)).collect()
}

/// The request setting the buildings selected by `mask` (bit i = i-th
/// building in ID order) to `level`.
fn masked(w: &ToyWorld, mask: u8, level: Lod) -> Vec<(EntityId, Lod)> {
    w.buildings()
        .ids()
        .iter()
        .enumerate()
        .filter(|(i, _)| mask >> i & 1 == 1)
        .map(|(_, b)| (*b, level))
        .collect()
}

/// An L1 walk without its derived velocity.
fn canonical(w: &Walk) -> (EntityId, bool, u32, u32) {
    (w.target, w.arrived, w.wait_steps, w.targets_drawn)
}

fn pos_bits(p: &SurfacePos) -> (u64, [u32; 3]) {
    (p.cell().raw(), p.offset().map(f32::to_bits))
}

/// The canonical L1 state of every L1 agent, bit for bit.
type L1State = BTreeMap<EntityId, ((u64, [u32; 3]), (EntityId, bool, u32, u32))>;

fn l1_state(w: &ToyWorld) -> L1State {
    w.walks()
        .iter()
        .map(|(id, walk)| {
            (
                id,
                (pos_bits(w.positions().get(id).unwrap()), canonical(walk)),
            )
        })
        .collect()
}

/// The next-target counter and target of each agent, whatever its level.
fn progress(w: &ToyWorld) -> BTreeMap<EntityId, (u32, EntityId)> {
    w.walks()
        .iter()
        .map(|(id, walk)| (id, (walk.targets_drawn, walk.target)))
        .chain(
            w.background()
                .iter()
                .map(|(id, bg)| (id, (bg.targets_drawn, bg.governing()))),
        )
        .collect()
}

#[test]
fn zero_time_round_trip_is_exact() {
    for seed in 0..6 {
        let mut s = Sim::toy(seed);
        for ticks in [0, 1, 600, 3_000] {
            s.run(ticks);
            let (hash, state, summary) =
                (s.w.state_hash(), l1_state(&s.w), s.w.conservation_summary());
            let walking = s.w.walks().iter().filter(|(_, w)| !w.arrived).count();
            let down = s.all(Lod::L2);
            assert_eq!(down.demoted, state.len());
            assert!(s.w.walks().is_empty());
            let up = s.all(Lod::L1);
            assert_eq!(up.promoted, state.len());
            assert_eq!(s.w.state_hash(), hash, "seed {seed} after {} ticks", s.tick);
            assert_eq!(l1_state(&s.w), state);
            assert_eq!(s.w.conservation_summary(), summary);
            if ticks >= 600 {
                assert!(walking > 0, "the round trip covered walking agents");
            }
        }
    }
}

#[test]
fn zero_time_round_trip_of_single_buildings_is_exact() {
    let mut s = Sim::toy(3);
    s.run(2_500);
    let hash = s.w.state_hash();
    for b in s.w.buildings().ids().to_vec() {
        s.set(&[(b, Lod::L2)]);
        s.set(&[(b, Lod::L1)]);
        assert_eq!(s.w.state_hash(), hash);
    }
}

#[test]
fn restating_a_level_is_a_no_op() {
    let mut s = Sim::toy(4);
    s.run(1_000);
    let hash = s.w.state_hash();
    assert_eq!(s.all(Lod::L1), TransitionReport::default());
    assert_eq!(s.w.state_hash(), hash);
    s.all(Lod::L2);
    let hash = s.w.state_hash();
    assert_eq!(s.all(Lod::L2), TransitionReport::default());
    assert_eq!(s.w.state_hash(), hash);
}

proptest! {
    #![proptest_config(config())]

    /// `A(D(x)) == x` for counted quantities at any time and for any subset
    /// of buildings: IDs, population, homes, governing buildings, counters,
    /// and the whole state hash survive L1 → L2 → L1 at one boundary.
    #[test]
    fn round_trip_conserves_ids_and_counts(
        seed in 0u64..1_000,
        ticks in 0i64..3_000,
        mask in any::<u8>(),
    ) {
        let mut s = Sim::toy(seed);
        s.run(ticks);
        let before = s.w.conservation_summary();
        let hash = s.w.state_hash();
        let down = masked(&s.w, mask, Lod::L2);
        s.set(&down);
        let mid = s.w.conservation_summary();
        prop_assert_eq!(before.compare_transaction(&mid), vec![]);
        prop_assert_eq!(&before, &mid);
        let up = masked(&s.w, mask, Lod::L1);
        s.set(&up);
        prop_assert_eq!(&s.w.conservation_summary(), &before);
        prop_assert_eq!(s.w.state_hash(), hash);
    }

    /// Random sequences of steps and requests never lose or duplicate an
    /// agent: after every transaction and every tick the stores partition
    /// the residents, and the checker finds nothing.
    #[test]
    fn repeated_cycles_never_lose_or_duplicate(
        seed in 0u64..1_000,
        ops in prop::collection::vec((0i64..60, any::<u8>(), any::<bool>()), 1..50),
    ) {
        let mut s = Sim::new(toy_with(seed, Population::TOY, WalkParams { speed_m_s: 6.0, wait_steps: 20 }));
        let population = s.w.residents().len();
        let ids: BTreeSet<EntityId> = s.w.residents().ids().iter().copied().collect();
        for (ticks, mask, l2) in ops {
            for _ in 0..ticks {
                s.tick();
                s.check();
                prop_assert_eq!(s.w.walks().len() + s.w.background().len(), population);
            }
            let req = masked(&s.w, mask, if l2 { Lod::L2 } else { Lod::L1 });
            s.set(&req);
            prop_assert_eq!(s.w.walks().len() + s.w.background().len(), population);
            let held: BTreeSet<EntityId> = s.w.walks().ids().iter().chain(s.w.background().ids()).copied().collect();
            prop_assert_eq!(&held, &ids);
        }
    }

    /// The same set of node levels requested in a different order gives
    /// the same world.
    #[test]
    fn request_order_does_not_matter(
        seed in 0u64..1_000,
        ticks in 0i64..2_000,
        levels in prop::collection::vec(any::<bool>(), 5),
        order in Just(()).prop_perturb(|_, mut rng| { let mut v: Vec<usize> = (0..5).collect(); for i in (1..5).rev() { v.swap(i, (rng.next_u32() as usize) % (i + 1)); } v }),
    ) {
        let mut s = Sim::toy(seed);
        s.run(ticks);
        let b = s.w.buildings().ids().to_vec();
        let req: Vec<(EntityId, Lod)> = (0..5).map(|i| (b[i], if levels[i] { Lod::L2 } else { Lod::L1 })).collect();
        let shuffled: Vec<(EntityId, Lod)> = order.iter().map(|&i| req[i]).collect();
        let mut a = s.clone();
        a.set(&req);
        s.set(&shuffled);
        prop_assert_eq!(a.w.state_hash(), s.w.state_hash());
        prop_assert_eq!(&a.w, &s.w);
        a.run(300);
        s.run(300);
        prop_assert_eq!(a.w.state_hash(), s.w.state_hash());
    }
}

#[test]
fn population_is_constant_across_stores() {
    let mut s = Sim::toy(12);
    let n = s.w.residents().len();
    for k in 0..6_000 {
        if k % 97 == 0 {
            let req = masked(
                &s.w,
                (k / 97 % 32) as u8,
                if k % 2 == 0 { Lod::L2 } else { Lod::L1 },
            );
            s.set(&req);
        }
        s.tick();
        assert_eq!(s.w.walks().len() + s.w.background().len(), n);
        assert_eq!(s.w.positions().len(), s.w.walks().len());
        assert_eq!(s.w.event_queue().len(), s.w.background().len());
    }
}

#[test]
fn identity_survives_every_transition() {
    let mut s = Sim::toy(13);
    let identity: Vec<(EntityId, Resident)> =
        s.w.residents().iter().map(|(i, r)| (i, *r)).collect();
    for k in 0..4_000 {
        if k % 50 == 0 {
            let req = masked(
                &s.w,
                (k / 50 % 32) as u8,
                if k % 100 == 0 { Lod::L2 } else { Lod::L1 },
            );
            s.set(&req);
        }
        s.tick();
    }
    let after: Vec<(EntityId, Resident)> = s.w.residents().iter().map(|(i, r)| (i, *r)).collect();
    assert_eq!(after, identity);
    for (id, r) in &after {
        assert_eq!(resident_id(13, s.w.dataset_version(), r.address), *id);
        assert!(s.w.residency(*id).is_some());
    }
}

/// Each agent's sequence of targets: the k-th entry is draw k of its
/// stream, whatever level made it.
fn record_targets(w: &ToyWorld, seq: &mut BTreeMap<EntityId, Vec<EntityId>>) {
    let walking_l1 = w
        .walks()
        .iter()
        .filter(|(_, walk)| !walk.arrived)
        .map(|(id, walk)| (id, walk.targets_drawn, walk.target));
    let walking_l2 = w
        .background()
        .iter()
        .filter_map(|(id, bg)| match bg.activity {
            Activity::Walking { dest, .. } => Some((id, bg.targets_drawn, dest)),
            Activity::Waiting { .. } => None,
        });
    for (id, drawn, target) in walking_l1.chain(walking_l2) {
        let s = seq.entry(id).or_default();
        if s.len() < drawn as usize {
            s.push(target);
        }
    }
}

#[test]
fn lod_does_not_perturb_target_sequences() {
    let walk = WalkParams {
        speed_m_s: 20.0,
        wait_steps: 10,
    };
    let mut base = Sim::new(toy_with(42, Population::TOY, walk));
    let mut reference = BTreeMap::new();
    for _ in 0..6_000 {
        base.tick();
        record_targets(&base.w, &mut reference);
    }
    let mut s = Sim::new(toy_with(42, Population::TOY, walk));
    let mut seq = BTreeMap::new();
    for k in 0..6_000u64 {
        if k % 37 == 0 {
            let mask = ws_rng::hash(1, 2, &[k]) as u8;
            let req = masked(&s.w, mask, if k % 74 == 0 { Lod::L2 } else { Lod::L1 });
            s.set(&req);
        }
        s.tick();
        // The L2 drain can make several draws in one tick; a one-tick
        // window records only the last, so also record at L1 every tick.
        record_targets(&s.w, &mut seq);
    }
    // Draws made and consumed within one L2 drain are not observed by the
    // recorder, so compare every draw both runs observed, by index.
    let draws = |w: &ToyWorld| progress(w);
    let (a, b) = (draws(&base.w), draws(&s.w));
    for (id, r) in &reference {
        let x = &seq[id];
        let n = r.len().min(x.len());
        assert!(n >= 3, "agent {id} made {n} trips");
        assert_eq!(&r[..n], &x[..n], "agent {id}");
        assert!(
            a[id].0.abs_diff(b[id].0) <= 1,
            "agent {id}: {:?} vs {:?}",
            a[id],
            b[id]
        );
    }
}

#[test]
fn every_l2_draw_is_the_l1_draw() {
    // Directly: an L2 agent drained over many legs makes exactly the draws
    // `next_target` defines for its counter, starting where L1 left off.
    let mut s = Sim::new(toy_with(
        77,
        Population::TOY,
        WalkParams {
            speed_m_s: 30.0,
            wait_steps: 5,
        },
    ));
    s.run(400);
    s.all(Lod::L2);
    let buildings = s.w.buildings().ids().to_vec();
    let mut last: BTreeMap<EntityId, Background> =
        s.w.background().iter().map(|(i, b)| (i, *b)).collect();
    for _ in 0..3_000 {
        s.tick();
        for (id, bg) in s.w.background().iter() {
            let prev = last.insert(id, *bg).unwrap();
            if let (Activity::Waiting { at, .. }, Activity::Walking { dest, origin, .. }) =
                (prev.activity, bg.activity)
            {
                assert_eq!(bg.targets_drawn, prev.targets_drawn + 1);
                assert_eq!(origin, Origin::Building(at));
                assert_eq!(
                    dest,
                    ws_world::ids::next_target(77, id, prev.targets_drawn, &buildings, at)
                );
            }
        }
    }
}

/// ADR 0017 §5 and §14: L2 for `hours`, against L1 for the same time.
fn l2_matches_l1(seed: u64, hours: i64) {
    let mut l1 = Sim::toy(seed);
    let centres: BTreeMap<EntityId, SurfacePos> =
        l1.w.buildings().iter().map(|(i, b)| (i, b.pos)).collect();
    let mut l2 = l1.clone();
    l2.all(Lod::L2);
    for _ in 0..hours * HOUR {
        l1.w.step(STEP.step()).unwrap();
        l1.tick += 1;
        l2.tick();
    }
    l2.all(Lod::L1);
    assert_eq!(l1.tick, l2.tick);
    let (mut same, mut walking, mut max_err) = (0, 0, 0.0_f64);
    for (id, a) in l1.w.walks().iter() {
        let b = l2.w.walks().get(id).unwrap();
        assert_eq!(a.targets_drawn, b.targets_drawn, "agent {id}");
        assert_eq!(a.target, b.target, "agent {id}");
        let pb = *l2.w.positions().get(id).unwrap();
        assert_eq!(pb.occupied_cell(), Ok(pb.cell()));
        if canonical(a) == canonical(b) {
            same += 1;
        }
        if b.arrived {
            // Exactly at its building's centre: "in the building its
            // schedule says".
            assert_eq!(pos_bits(&pb), pos_bits(&centres[&b.target]), "agent {id}");
        } else {
            walking += 1;
            let pa = *l1.w.positions().get(id).unwrap();
            let e = (pa.to_ecef() - pb.to_ecef()).length();
            max_err = max_err.max(e);
            assert!(e <= 0.1, "agent {id} is {e} m from its L1 position");
        }
    }
    let n = l1.w.walks().len();
    println!(
        "seed {seed}, {hours} h: {same}/{n} agents with identical discrete state, \
         {walking} walking, largest position difference {:.1} mm",
        max_err * 1e3
    );
    assert_eq!(same, n, "every leg started at a building, so timing agrees");
}

#[test]
fn l2_matches_l1_over_one_hour() {
    for seed in [1, 2] {
        l2_matches_l1(seed, 1);
    }
}

#[test]
#[ignore = "long-running (8 simulated hours of L1 reference): nightly, run with --release -- --ignored"]
fn l2_matches_l1_over_eight_hours() {
    for seed in [1, 2] {
        l2_matches_l1(seed, 8);
    }
}

#[test]
fn long_advance_equals_tick_by_tick_drain() {
    let mut base = Sim::toy(21);
    base.run(200);
    base.all(Lod::L2);
    let to = base.tick + 2 * HOUR;
    let mut one = base.w.clone();
    one.advance_background(t(to), STEP).unwrap();
    for chunk in [1, 7, 600] {
        let mut w = base.w.clone();
        let mut k = base.tick;
        while k < to {
            k = (k + chunk).min(to);
            w.advance_background(t(k), STEP).unwrap();
        }
        assert_eq!(w.state_hash(), one.state_hash(), "chunk {chunk}");
        assert_eq!(w, one);
    }
    // Ticking (with reconciliation, which finds nothing to do when every
    // building is at L2) gives the same.
    let mut ticked = base.clone();
    ticked.run(to - base.tick);
    assert_eq!(ticked.w.state_hash(), one.state_hash());
    // Agents crossed several arrive/wait/depart cycles in the one call.
    let legs: Vec<u32> = one
        .background()
        .iter()
        .map(|(id, bg)| bg.targets_drawn - base.w.background().get(id).unwrap().targets_drawn)
        .collect();
    assert!(legs.iter().all(|n| *n >= 3), "{legs:?}");
}

#[test]
fn one_step_legs_arrive_in_their_departure_step() {
    // A single building: every draw returns it, so every leg is empty.
    let site = Dataset::toy().unwrap().sites[0];
    let ds = Dataset {
        version: 901,
        sites: vec![site],
    };
    let walk = WalkParams {
        speed_m_s: 1.4,
        wait_steps: 3,
    };
    let w = ToyWorld::generate(5, &ds, Population::exactly(4), walk).unwrap();
    let mut l1 = Sim::new(w.clone());
    let mut l2 = Sim::new(w);
    l2.all(Lod::L2);
    for _ in 0..41 {
        l1.tick();
        l2.tick();
        for (_, bg) in l2.w.background().iter() {
            assert!(matches!(bg.activity, Activity::Waiting { .. }));
        }
    }
    l2.all(Lod::L1);
    assert_eq!(l1.w.state_hash(), l2.w.state_hash());
    // Departed every 4 ticks from tick 1: 11 draws in 41 ticks.
    assert!(l2.w.walks().iter().all(|(_, w)| w.targets_drawn == 11));
}

type Leg = (EntityId, Position<Ecef>, Position<Ecef>, i64, i64);

#[test]
fn mid_trip_promotion_is_on_the_leg() {
    let mut s = Sim::toy(31);
    s.all(Lod::L2);
    s.tick(); // every agent departs from home
    let step_m = 1.4 * 0.1;
    // (agent, origin, destination, origin tick, arrival tick)
    let legs: Vec<Leg> =
        s.w.background()
            .iter()
            .filter_map(|(id, bg)| match bg.activity {
                Activity::Walking {
                    origin: Origin::Building(o),
                    origin_time,
                    dest,
                    arrive,
                } => Some((
                    id,
                    s.w.buildings().get(o).unwrap().pos.to_ecef(),
                    s.w.buildings().get(dest).unwrap().pos.to_ecef(),
                    origin_time.as_micros_since_epoch() / 100_000,
                    arrive.as_micros_since_epoch() / 100_000,
                )),
                Activity::Walking { .. } | Activity::Waiting { .. } => None,
            })
            .collect();
    assert!(legs.len() > 10);
    let mut checked = 0;
    while s.tick < 1_500 {
        let mut up = s.clone();
        up.all(Lod::L1);
        for (id, o, g, t0, arrive) in &legs {
            let m = s.tick - t0;
            if m >= arrive - t0 {
                continue;
            }
            let p = *up.w.positions().get(*id).unwrap();
            assert_eq!(p.occupied_cell(), Ok(p.cell()), "{id}");
            let d = *g - *o;
            let expected = *o + d * (m as f64 * step_m / d.length());
            let e = (p.to_ecef() - expected).length();
            assert!(e < 1e-3, "{id} at step {m}: {e} m off the leg");
            assert!(!up.w.walks().get(*id).unwrap().arrived);
            checked += 1;
        }
        for _ in 0..37 {
            s.tick();
        }
    }
    assert!(checked > 500, "{checked}");
}

fn shuffle<T>(v: &mut [T], seed: u64) {
    for i in (1..v.len()).rev() {
        let j = (ws_rng::hash(seed, 9, &[i as u64]) % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
}

/// Rebuilds `w` by inserting its entities in an order given by `seed`.
fn rebuilt(w: &ToyWorld, seed: u64) -> ToyWorld {
    let mut v = ToyWorld::empty(w.seed(), w.dataset_version(), WalkParams::DEFAULT);
    let mut buildings: Vec<_> = w.buildings().iter().map(|(i, b)| (i, *b)).collect();
    shuffle(&mut buildings, seed);
    for (i, b) in buildings {
        v.insert_building(i, b).unwrap();
    }
    let mut levels: Vec<(EntityId, Lod)> = w.node_lods().iter().map(|(i, l)| (i, *l)).collect();
    shuffle(&mut levels, seed ^ 1);
    // No residents yet, so this only sets levels.
    assert_eq!(
        v.set_lod(t(0), STEP, &levels).unwrap(),
        TransitionReport::default()
    );
    let mut residents: Vec<EntityId> = w.residents().ids().to_vec();
    shuffle(&mut residents, seed ^ 2);
    for id in residents {
        let r = *w.residents().get(id).unwrap();
        if let Some(walk) = w.walks().get(id) {
            v.insert_resident_state(id, r, *w.positions().get(id).unwrap(), *walk)
                .unwrap();
        } else {
            v.insert_resident_background(id, r, *w.background().get(id).unwrap())
                .unwrap();
        }
    }
    v
}

#[test]
fn insertion_and_request_order_do_not_matter() {
    let mut s = Sim::toy(23);
    s.run(1_200);
    s.set(&masked(&s.w.clone(), 0b10110, Lod::L2));
    assert!(!s.w.walks().is_empty() && !s.w.background().is_empty());
    for order in 0..20 {
        let mut r = Sim {
            w: rebuilt(&s.w, order),
            tick: s.tick,
        };
        assert_eq!(r.w.state_hash(), s.w.state_hash(), "order {order}");
        let mut a = s.clone();
        for _ in 0..200 {
            a.tick();
            r.tick();
            assert_eq!(r.w.state_hash(), a.w.state_hash());
        }
    }
}

/// A point on an edge of `cell`, found by bisection along a ray from its
/// centre (as in tests/world.rs).
fn edge_point(cell: CellId, angle: f64) -> Position<Ecef> {
    let f = cell.enu_frame();
    let occ = |r: f64| {
        let p = f.to_ecef(Position::new(r * angle.cos(), r * angle.sin(), 0.0));
        CellId::from_ecef(p).unwrap().parent(SECTOR_LEVEL).unwrap()
    };
    let (mut lo, mut hi) = (0.0_f64, 2000.0_f64);
    for _ in 0..80 {
        let m = 0.5 * (lo + hi);
        if occ(m) == cell {
            lo = m;
        } else {
            hi = m;
        }
    }
    f.to_ecef(Position::new(lo * angle.cos(), lo * angle.sin(), 0.0))
}

#[test]
fn cell_edges_do_not_oscillate() {
    let home =
        CellId::from_geodetic_at_level(&ws_math::reference::BAY_RIDGE, SECTOR_LEVEL).unwrap();
    let sites: Vec<BuildingSite> = [0.3, 1.9, 3.5, 5.0]
        .iter()
        .map(|&a| BuildingSite {
            pos: SurfacePos::from_ecef(edge_point(home, a)).unwrap(),
            half_extent_m: 5.0,
            height_m: 5.0,
        })
        .collect();
    let ds = Dataset {
        version: 902,
        sites,
    };
    let w = ToyWorld::generate(
        1,
        &ds,
        Population::exactly(6),
        WalkParams {
            speed_m_s: 1.4,
            wait_steps: 100,
        },
    )
    .unwrap();
    let mut s = Sim::new(w);
    // Per agent: the cells visited on the current leg, and the last one.
    let mut leg_cells: BTreeMap<EntityId, (u32, Vec<CellId>)> = BTreeMap::new();
    let mut parked = 0;
    let mut last: BTreeMap<EntityId, SurfacePos> =
        s.w.positions().iter().map(|(i, p)| (i, *p)).collect();
    for k in 0..12_000 {
        // Demote and promote everything at every boundary.
        s.all(Lod::L2);
        s.all(Lod::L1);
        if k % 2 == 0 {
            s.all(Lod::L2);
        }
        s.tick();
        let mut up = s.clone();
        up.all(Lod::L1);
        for (id, p) in up.w.positions().iter() {
            assert_eq!(p.occupied_cell(), Ok(p.cell()), "{id}");
            let walk = up.w.walks().get(id).unwrap();
            if let Some(before) = last.insert(id, *p)
                && before.to_ecef() == p.to_ecef()
            {
                assert_eq!(
                    pos_bits(&before),
                    pos_bits(p),
                    "{id} changed cell standing still"
                );
                parked += 1;
            }
            let entry = leg_cells.entry(id).or_default();
            if entry.0 != walk.targets_drawn {
                *entry = (walk.targets_drawn, Vec::new());
            }
            if entry.1.last() != Some(&p.cell()) {
                assert!(
                    !entry.1.contains(&p.cell()),
                    "{id} returned to cell {:?} within one leg",
                    p.cell()
                );
                entry.1.push(p.cell());
            }
        }
    }
    // Coverage, not a tolerance: every parked observation is a bit-exact
    // comparison. Six agents waiting 100 steps between legs of roughly
    // 2,000 steps give about 3,300 of them.
    assert!(parked > 3_000, "{parked}");
}

/// Every canonical LOD field changes the hash; the event queue and
/// velocity do not (ADR 0017 §11).
#[test]
fn state_hash_covers_lod_state() {
    let mut s = Sim::toy(41);
    s.run(1_000);
    s.set(&masked(&s.w.clone(), 0b00111, Lod::L2));
    let h = s.w.state_hash();
    let (id, bg) =
        s.w.background()
            .iter()
            .find(|(_, bg)| matches!(bg.activity, Activity::Walking { .. }))
            .map(|(i, b)| (i, *b))
            .unwrap();
    let other = s.w.buildings().ids()[4];
    let with_bg = |bg: Background| {
        let mut w = ToyWorld::empty(s.w.seed(), s.w.dataset_version(), WalkParams::DEFAULT);
        for (i, b) in s.w.buildings().iter() {
            w.insert_building(i, *b).unwrap();
        }
        let levels: Vec<_> = s.w.node_lods().iter().map(|(i, l)| (i, *l)).collect();
        w.set_lod(t(0), STEP, &levels).unwrap();
        for (i, r) in s.w.residents().iter() {
            if let Some(walk) = s.w.walks().get(i) {
                w.insert_resident_state(i, *r, *s.w.positions().get(i).unwrap(), *walk)
                    .unwrap();
            } else {
                let b = if i == id {
                    bg
                } else {
                    *s.w.background().get(i).unwrap()
                };
                w.insert_resident_background(i, *r, b).unwrap();
            }
        }
        w.state_hash()
    };
    assert_eq!(with_bg(bg), h);
    let Activity::Walking {
        origin,
        origin_time,
        dest,
        arrive,
    } = bg.activity
    else {
        unreachable!()
    };
    let later = |x: SimTime| x.checked_add(STEP.step()).unwrap();
    let variants = [
        Background {
            targets_drawn: bg.targets_drawn + 1,
            ..bg
        },
        Background {
            activity: Activity::Walking {
                origin,
                origin_time,
                dest: other,
                arrive,
            },
            ..bg
        },
        Background {
            activity: Activity::Walking {
                origin,
                origin_time: later(origin_time),
                dest,
                arrive,
            },
            ..bg
        },
        Background {
            activity: Activity::Walking {
                origin,
                origin_time,
                dest,
                arrive: later(arrive),
            },
            ..bg
        },
        Background {
            activity: Activity::Walking {
                origin: Origin::Building(other),
                origin_time,
                dest,
                arrive,
            },
            ..bg
        },
        Background {
            activity: Activity::Waiting {
                at: dest,
                depart: arrive,
            },
            ..bg
        },
    ];
    for (i, v) in variants.into_iter().enumerate() {
        assert_ne!(with_bg(v), h, "variant {i}");
    }
    // A Point keyframe's bits.
    if let Origin::Point { cell, offset } = origin {
        let moved = Origin::Point {
            cell,
            offset: [offset[0] + 0.001, offset[1], offset[2]],
        };
        assert_ne!(
            with_bg(Background {
                activity: Activity::Walking {
                    origin: moved,
                    origin_time,
                    dest,
                    arrive
                },
                ..bg
            }),
            h
        );
    }
    // A node level, and residency.
    let mut flipped = s.clone();
    let b = s.w.buildings().ids()[3];
    flipped.set(&[(b, Lod::L2)]);
    assert_ne!(flipped.w.state_hash(), h);
    // An empty building's level alone also counts.
    let mut empty = ToyWorld::empty(1, 1, WalkParams::DEFAULT);
    let site = Dataset::toy().unwrap().sites[0];
    empty
        .insert_building(
            EntityId::procedural(7),
            ws_world::Building {
                index: 0,
                pos: site.pos,
                half_extent_m: 1.0,
                height_m: 1.0,
            },
        )
        .unwrap();
    let h0 = empty.state_hash();
    empty
        .set_lod(t(0), STEP, &[(EntityId::procedural(7), Lod::L2)])
        .unwrap();
    assert_ne!(empty.state_hash(), h0);
}

/// Velocity is derived (ADR 0017 §10–11): worlds that differ only in
/// velocity hash identically and evolve identically, and a zero-time round
/// trip, which recomputes every walker's velocity, changes neither the hash
/// nor any later state.
#[test]
fn velocity_is_derived() {
    let mut s = Sim::toy(51);
    s.run(800);
    // Scramble every velocity.
    let mut scrambled = ToyWorld::empty(s.w.seed(), s.w.dataset_version(), WalkParams::DEFAULT);
    for (i, b) in s.w.buildings().iter() {
        scrambled.insert_building(i, *b).unwrap();
    }
    for (i, r) in s.w.residents().iter() {
        let walk = Walk {
            velocity: [9.0, -9.0, 3.0],
            ..*s.w.walks().get(i).unwrap()
        };
        scrambled
            .insert_resident_state(i, *r, *s.w.positions().get(i).unwrap(), walk)
            .unwrap();
    }
    let mut scrambled = Sim {
        w: scrambled,
        tick: s.tick,
    };
    assert_eq!(scrambled.w.state_hash(), s.w.state_hash());
    // Round-tripped: velocities recomputed.
    let mut tripped = s.clone();
    tripped.all(Lod::L2);
    tripped.all(Lod::L1);
    let differs = tripped
        .w
        .walks()
        .iter()
        .filter(|(i, w)| w.velocity != s.w.walks().get(*i).unwrap().velocity)
        .count();
    assert!(differs > 0, "the round trip recomputed some velocities");
    assert_eq!(tripped.w.state_hash(), s.w.state_hash());
    for _ in 0..3_000 {
        s.tick();
        scrambled.tick();
        tripped.tick();
        assert_eq!(scrambled.w.state_hash(), s.w.state_hash());
        assert_eq!(tripped.w.state_hash(), s.w.state_hash());
        assert_eq!(l1_state(&tripped.w), l1_state(&s.w));
    }
}

#[test]
fn requests_are_validated_atomically() {
    let mut s = Sim::toy(61);
    s.run(900);
    s.set(&masked(&s.w.clone(), 0b01001, Lod::L2));
    let before = s.w.clone();
    let b = s.w.buildings().ids().to_vec();
    let agent = s.w.residents().ids()[0];
    let now = s.now();
    let cases: Vec<(Vec<(EntityId, Lod)>, LodError)> = vec![
        (vec![(agent, Lod::L1)], LodError::UnknownNode(agent)),
        (
            vec![(b[0], Lod::L2), (b[1], Lod::L1), (b[0], Lod::L2)],
            LodError::DuplicateNode(b[0]),
        ),
        (
            vec![(b[2], Lod::L1), (b[2], Lod::L2)],
            LodError::DuplicateNode(b[2]),
        ),
        (vec![(b[1], Lod::L0)], LodError::UnsupportedLevel(Lod::L0)),
        (
            vec![(b[1], Lod::L2), (b[3], Lod::L3)],
            LodError::UnsupportedLevel(Lod::L3),
        ),
        (vec![(b[4], Lod::L7)], LodError::UnsupportedLevel(Lod::L7)),
    ];
    for (req, err) in cases {
        assert_eq!(s.w.set_lod(now, STEP, &req), Err(err), "{req:?}");
        assert_eq!(s.w, before);
    }
    let off = now.checked_add(SimDuration::from_micros(1)).unwrap();
    assert_eq!(
        s.w.set_lod(off, STEP, &[(b[0], Lod::L1)]),
        Err(LodError::OffGrid(off))
    );
    assert_eq!(s.w, before);
}

#[test]
fn a_failed_reconstruction_rejects_the_whole_transaction() {
    let mut s = Sim::toy(62);
    s.run(10);
    s.all(Lod::L2);
    // An extra L2 resident whose keyframe cannot be a position.
    let b = s.w.buildings().ids().to_vec();
    let home = b[0];
    let index = s.w.buildings().get(home).unwrap().index;
    let address = ws_world::ResidentAddress {
        building_index: index,
        slot: 500,
    };
    let id = resident_id(62, s.w.dataset_version(), address);
    let cell = s.w.buildings().get(home).unwrap().pos.cell();
    s.w.insert_resident_background(
        id,
        Resident { address, home },
        Background {
            activity: Activity::Walking {
                origin: Origin::Point {
                    cell,
                    offset: [f32::NAN, 0.0, 0.0],
                },
                origin_time: s.now(),
                dest: b[1],
                arrive: t(s.tick + 50),
            },
            targets_drawn: 1,
        },
    )
    .unwrap();
    let before = s.w.clone();
    let err = s.w.set_lod(s.now(), STEP, &all(&s.w, Lod::L1)).unwrap_err();
    assert_eq!(err, LodError::Coord(ws_math::CoordError::NonFinite));
    // NaN is not equal to itself, so compare the bit-exact renderings.
    assert_eq!(format!("{:?}", s.w), format!("{before:?}"));
    assert_eq!(s.w.state_hash(), before.state_hash());
    // Naming only the agent's governing building fails the same way.
    let err = s.w.set_lod(s.now(), STEP, &[(b[1], Lod::L1)]).unwrap_err();
    assert_eq!(err, LodError::Coord(ws_math::CoordError::NonFinite));
    assert_eq!(s.w.state_hash(), before.state_hash());
}

#[test]
fn arrived_agents_must_stand_at_their_centre() {
    let s = Sim::toy(63);
    let id = s.w.residents().ids()[0];
    let r = *s.w.residents().get(id).unwrap();
    let walk = *s.w.walks().get(id).unwrap();
    assert!(walk.arrived);
    let p = *s.w.positions().get(id).unwrap();
    let off = SurfacePos::new(p.cell(), [p.offset()[0], p.offset()[1] + 0.25, 0.0]).unwrap();
    let mut v = ToyWorld::empty(s.w.seed(), s.w.dataset_version(), WalkParams::DEFAULT);
    for (i, b) in s.w.buildings().iter() {
        v.insert_building(i, *b).unwrap();
    }
    let before = v.clone();
    assert_eq!(
        v.insert_resident_state(id, r, off, walk),
        Err(ws_world::WorldError::Lod(LodError::Invariant(
            ws_world::LodViolation::NotAtCentre(id)
        )))
    );
    assert_eq!(v, before);
    v.insert_resident_state(id, r, p, walk).unwrap();
}

/// A random request every 1–100 steps; returns the checker's findings
/// after every step (none expected).
fn soak(seed: u64, per_building: u32, ticks: i64) {
    let mut s = Sim::new(toy_with(
        seed,
        Population::exactly(per_building),
        WalkParams::DEFAULT,
    ));
    let n = s.w.residents().len();
    let summary = s.w.conservation_summary();
    let mut next = 1;
    let mut transitions = (0, 0);
    let mut k = 0u64;
    while s.tick < ticks {
        if s.tick == next {
            k += 1;
            let x = ws_rng::hash(seed, 3, &[k]);
            let req = masked(
                &s.w,
                x as u8,
                if x >> 8 & 1 == 1 { Lod::L2 } else { Lod::L1 },
            );
            let r = s.set(&req);
            transitions.0 += r.demoted;
            transitions.1 += r.promoted;
            next += 1 + (x >> 16) as i64 % 100;
        }
        s.tick();
        // Debug builds already run the full checker inside every tick;
        // release builds (the nightly soak) run it here.
        if cfg!(not(debug_assertions)) {
            s.check();
        }
        assert_eq!(s.w.walks().len() + s.w.background().len(), n);
    }
    let end = s.w.conservation_summary();
    assert_eq!(end.identities, summary.identities);
    assert_eq!(end.resident_ids, summary.resident_ids);
    assert_eq!(end.per_home, summary.per_home);
    println!(
        "soak seed {seed}: {n} agents, {ticks} ticks, {k} requests, {} demotions, {} promotions",
        transitions.0, transitions.1
    );
}

#[test]
fn lod_soak_short() {
    // 1,000 agents for 30 simulated minutes (ADR 0017 §14).
    soak(71, 200, HOUR / 2);
}

#[test]
#[ignore = "long-running (3,000 agents, 24 simulated hours): nightly, run with --release -- --ignored"]
fn lod_soak_long() {
    soak(72, 600, 24 * HOUR);
    l2_matches_l1(72, 1);
}

//! ADR 0039 resident faith and daily goals: pure rules. Nothing here touches
//! the simulation, the event store, persistence or FFI; callers feed in the
//! events a tick recorded and apply the returned state and facts.
use crate::SimTime;
use ws_ecs::EntityId;
use ws_events::Payload;
use ws_rng::{Purpose, SipHasher24, Stream};

/// Lowest faith.
pub const FAITH_MIN: i16 = -100;
/// Highest faith.
pub const FAITH_MAX: i16 = 100;
/// Most goals in one day.
pub const MAX_GOALS: usize = 3;
const DAY_US: i64 = 86_400_000_000;
const GOAL_REWARD: i32 = 10;

/// A daily goal template (ADR 0039 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GoalKind {
    /// Bring N people to a gathering.
    Gathering = 1,
    /// Bless N different people.
    BlessDistinct = 2,
    /// Cure someone with a blessing.
    Cure = 3,
    /// Keep N people inside during a storm.
    Storm = 4,
}
impl GoalKind {
    /// The kind for a saved tag, if any.
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Gathering),
            2 => Some(Self::BlessDistinct),
            3 => Some(Self::Cure),
            4 => Some(Self::Storm),
            _ => None,
        }
    }
}

/// Invalid saved god state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GodError {
    /// More than [`MAX_GOALS`] goals.
    TooManyGoals,
    /// More than three blessed residents.
    TooManyBlessed,
    /// Faith outside [`FAITH_MIN`]..=[`FAITH_MAX`].
    FaithRange,
    /// Resident IDs not strictly ascending.
    Order,
    /// Two goals of one day share a template.
    DuplicateKind,
    /// An open goal already at or past its target.
    Progress,
    /// A resident listed twice among the blessed.
    DuplicateBlessed,
}

/// One goal of the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Goal {
    /// Template.
    pub kind: GoalKind,
    /// Progress needed.
    pub target: u8,
    /// Progress so far today.
    pub progress: u8,
    /// Whether completed.
    pub done: bool,
}

const BLANK: Goal = Goal {
    kind: GoalKind::Cure,
    target: 1,
    progress: 0,
    done: false,
};

/// The goal set of one day plus the distinct blessed residents (kind 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Goals {
    /// Goal day.
    pub day: i64,
    len: u8,
    slots: [Goal; MAX_GOALS],
    blessed: [EntityId; MAX_GOALS],
    blessed_len: u8,
}
impl Goals {
    /// Saved goals for restore.
    pub fn from_parts(day: i64, goals: &[Goal], blessed: &[EntityId]) -> Result<Self, GodError> {
        if goals.len() > MAX_GOALS {
            return Err(GodError::TooManyGoals);
        }
        if blessed.len() > MAX_GOALS {
            return Err(GodError::TooManyBlessed);
        }
        for (i, g) in goals.iter().enumerate() {
            if goals[..i].iter().any(|o| o.kind == g.kind) {
                return Err(GodError::DuplicateKind);
            }
            // `plan` marks a goal done the moment progress reaches its target.
            if !g.done && g.progress >= g.target {
                return Err(GodError::Progress);
            }
            // ...so a done goal has reached it.
            if g.done && g.progress < g.target {
                return Err(GodError::Progress);
            }
        }
        for (i, id) in blessed.iter().enumerate() {
            if blessed[..i].contains(id) {
                return Err(GodError::DuplicateBlessed);
            }
        }
        let mut slots = [BLANK; MAX_GOALS];
        slots[..goals.len()].copy_from_slice(goals);
        let mut ids = [EntityId::procedural(0); MAX_GOALS];
        ids[..blessed.len()].copy_from_slice(blessed);
        Ok(Self {
            day,
            len: goals.len() as u8,
            slots,
            blessed: ids,
            blessed_len: blessed.len() as u8,
        })
    }
    /// The goals in slot order.
    pub fn goals(&self) -> &[Goal] {
        &self.slots[..usize::from(self.len)]
    }
    /// Distinct residents blessed today (kind 2), in arrival order.
    pub fn blessed(&self) -> &[EntityId] {
        &self.blessed[..usize::from(self.blessed_len)]
    }
}

/// Feasibility caps at the day boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// Most residents of any featured home within reach.
    pub gathering_reach: u32,
    /// Living residents.
    pub living: u32,
    /// Retained residents.
    pub residents: u32,
}

/// The goal day of `now`, counted from the schedule epoch (floors before it).
pub fn day_of(now: SimTime, epoch: SimTime) -> i64 {
    (now.as_micros_since_epoch() - epoch.as_micros_since_epoch()).div_euclid(DAY_US)
}

/// The goal set of `day` (ADR 0039 §2).
pub fn generate(seed: u64, day: i64, caps: Caps) -> Goals {
    let s = Stream::new(seed, Purpose::new("ws-sim/slice/goals"), &[day as u64]);
    let count = 1 + (s.draw(0) % 3) as usize;
    let mut order = [1u8, 2, 3, 4];
    for i in 0..3 {
        let j = (s.draw(1 + i as u64) % (4 - i as u64)) as usize;
        order.swap(i, i + j);
    }
    let mut goals = Vec::with_capacity(MAX_GOALS);
    let mut next = 4u64;
    for tag in order {
        if goals.len() == count {
            break;
        }
        let Some(kind) = GoalKind::from_tag(tag) else {
            continue;
        };
        // (cap, base, modulus) from the ADR table; target = min(base + r mod m, cap).
        let (cap, base, m) = match kind {
            GoalKind::Gathering => (caps.gathering_reach, 3, 3),
            GoalKind::BlessDistinct => (caps.living, 2, 2),
            GoalKind::Cure => (1, 1, 1),
            GoalKind::Storm => ((caps.residents / 5).max(1), 3, 4),
        };
        if cap == 0 {
            continue;
        }
        let r = s.draw(next);
        next += 1;
        let target = (base + r % m).min(u64::from(cap)).min(255) as u8;
        goals.push(Goal {
            kind,
            target,
            progress: 0,
            done: false,
        });
    }
    Goals::from_parts(day, &goals, &[]).expect("at most three goals")
}

/// A candidate's position in its commit batch and payload.
#[derive(Debug, Clone, Copy)]
pub struct Observed<'a> {
    /// Index in the batch.
    pub index: usize,
    /// The recorded payload.
    pub payload: &'a Payload,
}

/// A goal completed by `plan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalFact {
    /// Goal slot.
    pub slot: u8,
    /// Goal template.
    pub kind: GoalKind,
    /// Goal target.
    pub target: u8,
    /// Batch index of the candidate whose processing completed it.
    pub trigger: usize,
}

/// Resident faith plus the day's goals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct God {
    faith: Vec<(EntityId, i16)>,
    goals: Goals,
    gathering: Option<(EntityId, u8)>,
}

impl God {
    /// Faith 0 for each of `residents` (any order; stored ascending).
    pub fn new(residents: &[EntityId], goals: Goals) -> Self {
        let mut faith: Vec<_> = residents.iter().map(|&id| (id, 0i16)).collect();
        faith.sort_by_key(|(id, _)| *id);
        faith.dedup_by_key(|(id, _)| *id);
        Self {
            faith,
            goals,
            gathering: None,
        }
    }
    /// Saved state for restore; `gathering` is the current gathering's
    /// building and joins recorded today (controller ruling R1).
    pub fn from_parts(
        faith: Vec<(EntityId, i16)>,
        goals: Goals,
        gathering: Option<(EntityId, u8)>,
    ) -> Result<Self, GodError> {
        if faith
            .iter()
            .any(|(_, f)| !(FAITH_MIN..=FAITH_MAX).contains(f))
        {
            return Err(GodError::FaithRange);
        }
        if faith.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(GodError::Order);
        }
        Ok(Self {
            faith,
            goals,
            gathering,
        })
    }
    /// Faith per resident in ascending ID order.
    pub fn faith(&self) -> &[(EntityId, i16)] {
        &self.faith
    }
    /// The day's goals.
    pub fn goals(&self) -> &Goals {
        &self.goals
    }
    /// The gathering being counted: building and joins recorded today.
    pub fn gathering(&self) -> Option<(EntityId, u8)> {
        self.gathering
    }
    /// Mean faith, truncated toward zero; 0 with no residents.
    pub fn block_faith(&self) -> i32 {
        if self.faith.is_empty() {
            return 0;
        }
        let sum: i64 = self.faith.iter().map(|(_, f)| i64::from(*f)).sum();
        (sum / self.faith.len() as i64) as i32
    }
    /// Whether any faith, progress or completion differs from a fresh day.
    ///
    /// Ignoring `blessed` and `gathering` is safe: each only matters to a goal
    /// that would already show progress > 0 (or done), and both reset at `roll_day`.
    pub fn touched(&self) -> bool {
        self.faith.iter().any(|(_, f)| *f != 0)
            || self.goals.goals().iter().any(|g| g.progress > 0 || g.done)
    }
    /// Canonical state hash.
    pub fn hash(&self) -> u64 {
        let mut h = SipHasher24::new(0, Purpose::new("ws-sim/slice-god/v1").value());
        h.write_words(&[self.faith.len() as u64]);
        for (id, f) in &self.faith {
            h.write_words(&[id.raw(), i64::from(*f) as u64]);
        }
        h.write_words(&[self.goals.day as u64, u64::from(self.goals.len)]);
        for g in self.goals.goals() {
            h.write_words(&[
                g.kind as u64,
                u64::from(g.target),
                u64::from(g.progress),
                u64::from(g.done),
            ]);
        }
        h.write_words(&[u64::from(self.goals.blessed_len)]);
        for id in self.goals.blessed() {
            h.write_words(&[id.raw()]);
        }
        match self.gathering {
            Some((b, n)) => h.write_words(&[1, b.raw(), u64::from(n)]),
            None => h.write_words(&[0]),
        }
        h.finish()
    }

    fn add(&mut self, id: EntityId, delta: i32) {
        if let Ok(i) = self.faith.binary_search_by_key(&id, |(r, _)| *r) {
            let v = i32::from(self.faith[i].1) + delta;
            self.faith[i].1 = v.clamp(i32::from(FAITH_MIN), i32::from(FAITH_MAX)) as i16;
        }
    }
    fn raise(&mut self, kind: GoalKind, f: impl Fn(&Goal) -> u8) {
        for g in &mut self.goals.slots[..usize::from(self.goals.len)] {
            if g.kind == kind && !g.done {
                g.progress = g.progress.max(f(g));
            }
        }
    }

    /// Apply the §1 rules and goal progress in batch order. `cure_residents`
    /// are residents whose `Recovered` in this batch is caused by a bless.
    pub fn plan(&self, batch: &[Observed], cure_residents: &[EntityId]) -> (God, Vec<GoalFact>) {
        let mut next = self.clone();
        let mut facts = Vec::new();
        for o in batch {
            let mut progressed = [false; 5];
            match *o.payload {
                Payload::BlessApplied { resident, .. } => {
                    next.add(resident, 15);
                    if !next.goals.blessed().contains(&resident)
                        && usize::from(next.goals.blessed_len) < MAX_GOALS
                    {
                        let n = usize::from(next.goals.blessed_len);
                        next.goals.blessed[n] = resident;
                        next.goals.blessed_len += 1;
                    }
                    let n = next.goals.blessed_len;
                    next.raise(GoalKind::BlessDistinct, |_| n);
                    progressed[2] = true;
                }
                Payload::SliceIllnessApplied {
                    resident, affected, ..
                } => {
                    if affected == 1 {
                        next.add(resident, -20);
                    }
                }
                Payload::StayedInside { resident, .. } => next.add(resident, -5),
                Payload::JoinedGathering { resident, building } => {
                    next.add(resident, 10);
                    let joined = match next.gathering {
                        Some((b, n)) if b == building => n.saturating_add(1),
                        _ => 1,
                    };
                    next.gathering = Some((building, joined));
                    next.raise(GoalKind::Gathering, |_| joined);
                    progressed[1] = true;
                }
                Payload::GatheringEnded { building, .. } => {
                    if matches!(next.gathering, Some((b, _)) if b == building) {
                        next.gathering = None;
                    }
                }
                Payload::StormKeptInside { count } => {
                    let c = count.min(255) as u8;
                    next.raise(GoalKind::Storm, |_| c);
                    progressed[4] = true;
                }
                Payload::Recovered { resident, .. } if cure_residents.contains(&resident) => {
                    next.raise(GoalKind::Cure, |g| g.progress.saturating_add(1));
                    progressed[3] = true;
                }
                _ => {}
            }
            if !progressed.iter().any(|p| *p) {
                continue;
            }
            let mut completed = 0;
            for slot in 0..usize::from(next.goals.len) {
                let g = &mut next.goals.slots[slot];
                if !g.done && g.progress >= g.target {
                    g.done = true;
                    facts.push(GoalFact {
                        slot: slot as u8,
                        kind: g.kind,
                        target: g.target,
                        trigger: o.index,
                    });
                    completed += 1;
                }
            }
            for _ in 0..completed {
                for (_, f) in &mut next.faith {
                    *f = (i32::from(*f) + GOAL_REWARD)
                        .clamp(i32::from(FAITH_MIN), i32::from(FAITH_MAX))
                        as i16;
                }
            }
        }
        (next, facts)
    }

    /// A new day: `fresh` goals replace the old, faith stays, and the
    /// gathering count restarts (R1).
    pub fn roll_day(&self, day: i64, fresh: Goals) -> God {
        debug_assert_eq!(fresh.day, day);
        God {
            faith: self.faith.clone(),
            goals: fresh,
            gathering: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 71449;
    const CAPS: Caps = Caps {
        gathering_reach: 7,
        living: 30,
        residents: 30,
    };
    fn id(n: u64) -> EntityId {
        EntityId::procedural(n.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }
    fn ids() -> Vec<EntityId> {
        (1..=5).map(id).collect()
    }
    fn bless(r: EntityId) -> Payload {
        Payload::BlessApplied {
            resident: r,
            sequence: 0,
            affected: 1,
        }
    }
    fn illness(r: EntityId, affected: u32) -> Payload {
        Payload::SliceIllnessApplied {
            resident: r,
            sequence: 0,
            severity: 1,
            duration_us: 1,
            affected,
        }
    }
    fn joined(r: EntityId, b: EntityId) -> Payload {
        Payload::JoinedGathering {
            resident: r,
            building: b,
        }
    }
    fn obs(ps: &[Payload]) -> Vec<Observed<'_>> {
        ps.iter()
            .enumerate()
            .map(|(index, payload)| Observed { index, payload })
            .collect()
    }
    fn goals_of(kinds: &[(GoalKind, u8)]) -> Goals {
        let g: Vec<Goal> = kinds
            .iter()
            .map(|&(kind, target)| Goal {
                kind,
                target,
                progress: 0,
                done: false,
            })
            .collect();
        Goals::from_parts(0, &g, &[]).unwrap()
    }
    fn faith_of(g: &God, r: EntityId) -> i16 {
        g.faith().iter().find(|(i, _)| *i == r).unwrap().1
    }

    #[test]
    fn generate_is_deterministic_and_bounded() {
        for day in -2..=30 {
            let a = generate(SEED, day, CAPS);
            assert_eq!(a, generate(SEED, day, CAPS));
            let gs = a.goals();
            assert!((1..=3).contains(&gs.len()));
            for (i, g) in gs.iter().enumerate() {
                assert!(gs[..i].iter().all(|o| o.kind != g.kind));
                let range = match g.kind {
                    GoalKind::Gathering => 3..=5,
                    GoalKind::BlessDistinct => 2..=3,
                    GoalKind::Cure => 1..=1,
                    GoalKind::Storm => 3..=6,
                };
                assert!(range.contains(&g.target), "{g:?}");
                assert!(g.target <= 7);
            }
        }
    }

    #[test]
    fn generate_golden_days() {
        // Frozen from the first green run; any change is a determinism break.
        let got: Vec<Vec<(u8, u8)>> = (0..3)
            .map(|d| {
                generate(SEED, d, CAPS)
                    .goals()
                    .iter()
                    .map(|g| (g.kind as u8, g.target))
                    .collect()
            })
            .collect();
        let want: Vec<Vec<(u8, u8)>> =
            vec![vec![(2, 2)], vec![(1, 4), (4, 5), (2, 3)], vec![(2, 2)]];
        assert_eq!(got, want);
    }

    #[test]
    fn faith_rules() {
        let (a, b, c, d, e) = (id(1), id(2), id(3), id(4), id(5));
        let home = id(99);
        let ps = [
            bless(a),
            illness(b, 1),
            illness(c, 0),
            Payload::StayedInside {
                resident: d,
                building: home,
                activity: ws_life::Activity::Work,
            },
            Payload::StayedInside {
                resident: d,
                building: home,
                activity: ws_life::Activity::Work,
            },
            joined(e, home),
        ];
        let god = God::new(&ids(), goals_of(&[(GoalKind::Storm, 3)]));
        let (n, facts) = god.plan(&obs(&ps), &[]);
        assert!(facts.is_empty());
        assert_eq!(
            [a, b, c, d, e].map(|r| faith_of(&n, r)),
            [15, -20, 0, -10, 10]
        );
        // (15 - 20 + 0 - 10 + 10) / 5 = -1
        assert_eq!(n.block_faith(), -1);
        assert!(n.touched());
        assert!(!god.touched());
    }

    #[test]
    fn repeat_bless_counts_once_and_faith_clamps() {
        let a = id(1);
        let ps: Vec<Payload> = (0..8).map(|_| bless(a)).collect();
        let god = God::new(&ids(), goals_of(&[(GoalKind::BlessDistinct, 3)]));
        let (n, _) = god.plan(&obs(&ps), &[]);
        assert_eq!(faith_of(&n, a), 100);
        assert_eq!(n.goals().goals()[0].progress, 1);
        assert_eq!(n.goals().blessed(), &[a]);
    }

    #[test]
    fn completion_emits_one_fact_and_raises_everyone() {
        let (a, b, c) = (id(1), id(2), id(3));
        let ps = [bless(a), bless(b), bless(c)];
        let god = God::new(&ids(), goals_of(&[(GoalKind::BlessDistinct, 2)]));
        let (n, facts) = god.plan(&obs(&ps), &[]);
        assert_eq!(
            facts,
            vec![GoalFact {
                slot: 0,
                kind: GoalKind::BlessDistinct,
                target: 2,
                trigger: 1
            }]
        );
        assert!(n.goals().goals()[0].done);
        // Reward lands after the second bless: a 15+10, b 15+10, c 0+10 then +15 from its own bless.
        assert_eq!(faith_of(&n, a), 25);
        assert_eq!(faith_of(&n, b), 25);
        assert_eq!(faith_of(&n, c), 25);
        assert_eq!(faith_of(&n, id(4)), 10);
        // a third distinct bless after completion emits nothing more
        let (_, again) = n.plan(&obs(&[bless(id(4))]), &[]);
        assert!(again.is_empty());
    }

    #[test]
    fn gathering_goal_capped_and_skipped_when_unreachable() {
        for day in -2..=30 {
            let capped = generate(
                SEED,
                day,
                Caps {
                    gathering_reach: 2,
                    ..CAPS
                },
            );
            for g in capped
                .goals()
                .iter()
                .filter(|g| g.kind == GoalKind::Gathering)
            {
                assert_eq!(g.target, 2);
            }
            let none = generate(
                SEED,
                day,
                Caps {
                    gathering_reach: 0,
                    ..CAPS
                },
            );
            assert!(none.goals().iter().all(|g| g.kind != GoalKind::Gathering));
            assert!(!none.goals().is_empty());
        }
        assert!((-2..=30).any(|d| {
            generate(
                SEED,
                d,
                Caps {
                    gathering_reach: 2,
                    ..CAPS
                },
            )
            .goals()
            .iter()
            .any(|g| g.kind == GoalKind::Gathering)
        }));
    }

    #[test]
    fn capped_gathering_goal_completes_at_reach_two() {
        let home = id(99);
        let god = God::new(&ids(), goals_of(&[(GoalKind::Gathering, 2)]));
        let ps = [joined(id(1), home), joined(id(2), home)];
        let (n, facts) = god.plan(&obs(&ps), &[]);
        assert_eq!(
            facts,
            vec![GoalFact {
                slot: 0,
                kind: GoalKind::Gathering,
                target: 2,
                trigger: 1
            }]
        );
        assert!(n.goals().goals()[0].done);
    }

    #[test]
    fn day_of_floors_before_epoch() {
        let t = SimTime::from_micros_since_epoch;
        assert_eq!(day_of(t(-1), t(0)), -1);
        assert_eq!(day_of(t(0), t(0)), 0);
        assert_eq!(day_of(t(86_400_000_000), t(0)), 1);
    }

    #[test]
    fn gathering_progress_counts_one_building_and_resets() {
        let (b1, b2) = (id(90), id(91));
        let ps = [
            joined(id(1), b1),
            joined(id(2), b1),
            joined(id(3), b2),
            joined(id(4), b2),
            joined(id(5), b2),
        ];
        let god = God::new(&ids(), goals_of(&[(GoalKind::Gathering, 5)]));
        let (n, facts) = god.plan(&obs(&ps), &[]);
        assert!(facts.is_empty());
        assert_eq!(n.goals().goals()[0].progress, 3);
        assert_eq!(n.gathering(), Some((b2, 3)));
        let (n, _) = n.plan(
            &obs(&[Payload::GatheringEnded {
                building: b2,
                attendees: 3,
            }]),
            &[],
        );
        assert_eq!(n.gathering(), None);
        assert_eq!(n.goals().goals()[0].progress, 3);
        let (n, _) = n.plan(&obs(&[joined(id(1), b2)]), &[]);
        assert_eq!(n.gathering(), Some((b2, 1)));
        let rolled = n.roll_day(1, generate(SEED, 1, CAPS));
        assert_eq!(rolled.gathering(), None);
        assert_eq!(rolled.faith(), n.faith());
    }

    #[test]
    fn cure_and_storm_progress_and_from_parts_validation() {
        let a = id(1);
        let ps = [
            Payload::StormKeptInside { count: 2 },
            Payload::Recovered {
                resident: a,
                health: 100,
            },
            Payload::Recovered {
                resident: id(2),
                health: 100,
            },
        ];
        let god = God::new(
            &ids(),
            goals_of(&[(GoalKind::Cure, 1), (GoalKind::Storm, 3)]),
        );
        let (n, facts) = god.plan(&obs(&ps), &[a]);
        assert_eq!(facts.len(), 1);
        assert_eq!((facts[0].kind, facts[0].trigger), (GoalKind::Cure, 1));
        assert_eq!(n.goals().goals()[1].progress, 2);
        assert_ne!(god.hash(), n.hash());
        assert_eq!(
            Goals::from_parts(0, &[BLANK; 4], &[]),
            Err(GodError::TooManyGoals)
        );
        assert_eq!(
            Goals::from_parts(0, &[], &ids()[..4]),
            Err(GodError::TooManyBlessed)
        );
        let g = goals_of(&[]);
        assert_eq!(
            God::from_parts(vec![(id(1), 101)], g, None),
            Err(GodError::FaithRange)
        );
        let (lo, hi) = (id(1).min(id(2)), id(1).max(id(2)));
        assert_eq!(
            God::from_parts(vec![(hi, 0), (lo, 0)], g, None),
            Err(GodError::Order)
        );
        assert_eq!(
            God::from_parts(vec![(lo, 0), (lo, 0)], g, None),
            Err(GodError::Order)
        );
        assert!(God::from_parts(vec![(lo, -100), (hi, 100)], g, Some((id(9), 2))).is_ok());
    }

    #[test]
    fn goals_from_parts_refuses_inconsistent_state() {
        let goal = |kind, target, progress, done| Goal {
            kind,
            target,
            progress,
            done,
        };
        let bless = goal(GoalKind::BlessDistinct, 2, 1, false);
        assert_eq!(
            Goals::from_parts(0, &[bless, bless], &[]),
            Err(GodError::DuplicateKind)
        );
        assert_eq!(
            Goals::from_parts(0, &[goal(GoalKind::Storm, 3, 3, false)], &[]),
            Err(GodError::Progress)
        );
        assert_eq!(
            Goals::from_parts(0, &[goal(GoalKind::Storm, 3, 9, false)], &[]),
            Err(GodError::Progress)
        );
        assert_eq!(
            Goals::from_parts(0, &[bless], &[id(1), id(1)]),
            Err(GodError::DuplicateBlessed)
        );
        // A storm can overshoot its target in the batch that completes it.
        assert!(Goals::from_parts(0, &[goal(GoalKind::Storm, 3, 9, true)], &[]).is_ok());
        assert!(Goals::from_parts(0, &[bless], &[id(1)]).is_ok());
    }
}

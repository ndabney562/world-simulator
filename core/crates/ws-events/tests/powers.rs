//! ADR 0038 §5: tags 14 through 22, frozen payload words and root recognition.
use ws_ecs::EntityId;
use ws_events::*;
use ws_time::SimTime;

fn r() -> EntityId {
    EntityId::procedural(1)
}
fn b() -> EntityId {
    EntityId::procedural(2)
}
const D: u64 = 3_600_000_000;

fn all() -> [Payload; 9] {
    [
        Payload::BlessApplied {
            resident: r(),
            sequence: 3,
            affected: 1,
        },
        Payload::NeedsRelieved { resident: r() },
        Payload::StormApplied {
            sequence: 4,
            duration_us: D,
        },
        Payload::StayedInside {
            resident: r(),
            building: b(),
            activity: Activity::Work,
        },
        Payload::StormKeptInside { count: 5 },
        Payload::StormEnded { sequence: 4 },
        Payload::GatheringCalled {
            building: b(),
            sequence: 3,
            duration_us: D,
            attendees: 2,
        },
        Payload::JoinedGathering {
            resident: r(),
            building: b(),
        },
        Payload::GatheringEnded {
            building: b(),
            attendees: 2,
        },
    ]
}

#[test]
fn tags_append_after_13() {
    let tags: Vec<u32> = all().iter().map(|p| p.tag()).collect();
    assert_eq!(tags, (14..=22).collect::<Vec<u32>>());
    let names: Vec<&str> = all().iter().map(|p| p.name()).collect();
    assert_eq!(names[0], "intervention.bless_applied");
    assert_eq!(names[3], "life.stayed_inside");
    assert_eq!(names[8], "social.gathering_ended");
    assert_eq!(names[6], "intervention.gathering_called");
    assert_eq!(names[2], "intervention.storm_applied");
    assert_eq!(names[1], "life.needs_relieved");
    assert_eq!(names[4], "life.storm_kept_inside");
    assert_eq!(names[5], "life.storm_ended");
    assert_eq!(names[7], "social.joined_gathering");
}

#[test]
fn importance_follows_decision_d4() {
    let imp: Vec<u16> = all().iter().map(|p| p.importance()).collect();
    // Roots 32768; StayedInside 8192 (MissedDuty), JoinedGathering 8192
    // (NeighbourCheckIn); the rest reuse Recovered's 8192.
    assert_eq!(
        imp,
        vec![32768, 8192, 32768, 8192, 8192, 8192, 32768, 8192, 8192]
    );
    assert_eq!(
        all().map(|p| p.is_slice_root()),
        [true, false, true, false, false, false, true, false, false]
    );
    let illness = Payload::SliceIllnessApplied {
        resident: r(),
        sequence: 0,
        severity: 1,
        duration_us: 100_000,
        affected: 1,
    };
    assert!(illness.is_slice_root());
    assert!(
        !Payload::Recovered {
            resident: r(),
            health: 1
        }
        .is_slice_root()
    );
}

/// The payload words as visited by the digest, between the fixed header and
/// the trailing actor/location/cause/related counts (all zero here).
fn payload_words(p: Payload) -> Vec<u64> {
    let mut s = EventStore::new(vec![r(), b()]);
    let c = Candidate::new(
        SimTime::from_micros_since_epoch(1_000_000),
        10,
        0,
        p,
        match p {
            Payload::BlessApplied { sequence, .. }
            | Payload::StormApplied { sequence, .. }
            | Payload::GatheringCalled { sequence, .. } => Origin::Command(sequence),
            _ => Origin::System(80),
        },
    );
    s.prepare(1, &[c]).unwrap().commit();
    let mut words = Vec::new();
    s.records()[0].visit_words(|v| words.extend_from_slice(v));
    // 11 header words + 2 origin words; then 4 trailing zero counts.
    words[13..words.len() - 4].to_vec()
}

#[test]
fn projection_words_match_adr_table() {
    let (r, b) = (r().raw(), b().raw());
    let want: [Vec<u64>; 9] = [
        vec![r, 3, 1, 0, 0],
        vec![r, 0, 0, 0, 0],
        vec![4, D, 0, 0, 0],
        vec![r, b, Activity::Work as u64, 0, 0],
        vec![5, 0, 0, 0, 0],
        vec![4, 0, 0, 0, 0],
        vec![b, 3, D, 2, 0],
        vec![r, b, 0, 0, 0],
        vec![b, 2, 0, 0, 0],
    ];
    for (p, mut w) in all().into_iter().zip(want) {
        let got = payload_words(p);
        while w.last() == Some(&0) {
            w.pop();
        }
        // Trailing zero words are padding in the five-word projection.
        assert_eq!(got, w, "{}", p.name());
    }
}

#[test]
fn root_intervention_finds_new_roots() {
    let mut s = EventStore::new(vec![r(), b()]);
    let root = Candidate::new(
        SimTime::from_micros_since_epoch(1_000_000),
        10,
        0,
        Payload::StormApplied {
            sequence: 4,
            duration_us: D,
        },
        Origin::Command(4),
    );
    s.prepare(1, &[root]).unwrap().commit();
    let root_id = s.records()[0].id;
    let mut ended = Candidate::new(
        SimTime::from_micros_since_epoch(2_000_000),
        80,
        0,
        Payload::StormEnded { sequence: 4 },
        Origin::System(80),
    );
    ended
        .causes
        .push(Cause {
            target: Target::Id(root_id),
            relation: Relation::CausedBy,
            confidence: Confidence::Direct,
        })
        .unwrap();
    s.prepare(2, &[ended]).unwrap().commit();
    let ended_id = s.records()[1].id;
    assert_eq!(s.root_intervention(ended_id).map(|e| e.id), Some(root_id));
    assert_eq!(s.root_intervention(root_id).map(|e| e.id), Some(root_id));
}

/// ADR 0038 §2: a bless cure is a system-84 `Recovered` with exactly two causes,
/// `CausedBy/Direct` the bless root then `ContributedTo/Direct` the illness;
/// the natural system-82 recovery keeps its single `CausedBy/Direct` link.
#[test]
fn bless_recovery_takes_two_causes_and_natural_recovery_one() {
    let at = |s| SimTime::from_micros_since_epoch(s);
    let link = |id, relation| Cause {
        target: Target::Id(id),
        relation,
        confidence: Confidence::Direct,
    };
    let mut s = EventStore::new(vec![r(), b()]);
    let ill = Candidate::new(
        at(1_000_000),
        12,
        0,
        Payload::NeedsRelieved { resident: r() },
        Origin::System(12),
    );
    let bless = Candidate::new(
        at(1_000_000),
        10,
        1,
        Payload::BlessApplied {
            resident: r(),
            sequence: 1,
            affected: 1,
        },
        Origin::Command(1),
    );
    s.prepare(1, &[bless, ill]).unwrap().commit();
    let (root, ill) = (s.records()[0].id, s.records()[1].id);
    let recovered = |system, causes: &[Cause]| {
        let mut c = Candidate::new(
            at(2_000_000),
            system,
            r().raw(),
            Payload::Recovered {
                resident: r(),
                health: 100,
            },
            Origin::System(system),
        );
        c.lod = 1;
        c.actors
            .push(Actor {
                id: r(),
                role: Role::Subject,
            })
            .unwrap();
        for cause in causes {
            c.causes.push(*cause).unwrap();
        }
        c
    };
    let two = [
        link(root, Relation::CausedBy),
        link(ill, Relation::ContributedTo),
    ];
    let one = [link(ill, Relation::CausedBy)];
    let swapped = [
        link(ill, Relation::ContributedTo),
        link(root, Relation::CausedBy),
    ];
    for (system, causes, accepted) in [
        (84, &two[..], true),
        (82, &one[..], true),
        (82, &two[..], false),
        (84, &one[..], false),
        (84, &swapped[..], false),
        (83, &one[..], false),
    ] {
        let result = s.prepare(2, &[recovered(system, causes)]).map(|_| ());
        assert_eq!(
            result.is_ok(),
            accepted,
            "system {system} causes {causes:?}"
        );
    }
    s.prepare(2, &[recovered(84, &two)]).unwrap().commit();
    let id = s.records()[2].id;
    assert_eq!(s.root_intervention(id).map(|e| e.id), Some(root));
}

#[test]
fn goal_completed_tag_name_and_words() {
    for day in [-1i64, 7] {
        let p = Payload::GoalCompleted {
            day,
            slot: 2,
            kind: 3,
            target: 4,
        };
        assert_eq!(p.tag(), 23);
        assert_eq!(p.name(), "god.goal_completed");
        assert_eq!(p.importance(), 8192);
        assert!(!p.is_slice_root());
        assert_eq!(payload_words(p), vec![day as u64, 2, 3, 4]);

        let mut s = EventStore::new(vec![r(), b()]);
        let c = Candidate::new(
            SimTime::from_micros_since_epoch(1_000_000),
            87,
            0,
            p,
            Origin::System(87),
        );
        s.prepare(1, &[c]).unwrap().commit();
        let id = s.records()[0].id;
        assert!(s.root_intervention(id).is_none());
    }
}

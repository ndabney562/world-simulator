# ws-life

Owns immutable real-slice schedule profiles and pure L2 activity/location queries under [ADR 0027](../../../docs/adr/0027-slice-schedules-and-events.md). Dependencies point only to foundation/world/events; there is no simulation, persistence, platform or presentation dependency. All calculations are integer Tier 1, with rules version 1 and no mutable RNG, clock ownership, event-store reads or I/O.

`Schedules::new(&population, schedule_epoch)` uses an explicit Monday midnight in a seven-day **model phase**. VS-007 owns civil New York calendar alignment. Every household gets a fixed -15..=15 minute shift from draw zero of `ws-life/schedule/household-jitter`, addressed by household ID. This shifts every non-midnight boundary; intervals stay positive and half-open.

| Profile | Minutes after midnight before household shift |
|---|---|
| Employed weekday | 0 sleep; 390 home; 450 outbound commute; 480 work; 1020 return commute; 1050 home; 1080 shop commute; 1090 errands; 1120 home commute; 1130 home; 1320 sleep |
| Student weekday | Same, with school at 480, return commute at 900, home at 930 |
| Preschool/retired, or any weekend | 0 sleep; 480 home; 660 shop commute; 670 errands; 700 home commute; 710 home; 1320 sleep |

These are synthetic coarse timetables. Journey durations do not measure road distance, including travel between uses in one building. L1 paths/needs are #69. Off-map work/school retains workplace and exit IDs; the person is never removed. `Location` is one stationary place or one journey with both endpoints. Journey times use i128 absolute microseconds so an interval intersecting a SimTime endpoint retains exact scheduled endpoints. Ordinary queried/current time remains SimTime/i64; no saturation occurs.

`facts_at` plans typed `agent.schedule_changed` facts sorted by resident ID (producer 60, LOD2, subject resident/object destination, no invented causal links). The caller owns transactional event publication. Midnight does not invent sleep-to-sleep changes. Queries and next-boundary reconstruction do not mutate canonical state.

Tests compare every resident over a full week and before the epoch against independently written interval lists; exercise exact boundary ownership, negative/extreme times and off-map conservation; and freeze tables/draws in `tests/golden/schedules.txt`. The separate Python oracle `scripts/vs_schedule_golden.py` reads the previously frozen population fixture and independently computes the new counter draws/tables. Existing goldens remain unchanged. Run `cargo test -p ws-life` from `core/` and `python3 scripts/vs_schedule_golden.py check` from the repository root.

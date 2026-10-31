# ADR 0039: Resident faith and daily goals (GP-004)

- Status: Accepted
- Date: 2026-10-31
- Decided by: Noah, who approved the design and accepted this ADR in chat on 2026-10-31; proposed by Claude for issue #127
- Issue: #127 (GP-004). A feature issue, as with ADR 0038 (#125)
- Supersedes: none; extends ADR 0038's powers and ADR 0036's slice persistence

## Context

The slice has four god powers (ADR 0034 curse, ADR 0038 bless, storm and gathering) and the GP-003 reactions (#126) that show them land, but nothing gives the player a reason to cast one power rather than another, or to come back tomorrow. Issue #127 asks for a per-resident opinion of the god that rises with blessings and falls with curses, a block faith meter, and one to three daily goals generated from real state whose completion raises faith. Faith and goals must live in the Rust core, be saved, and be deterministic (AGENTS.md guardrails; ADR 0010 Tier 1).

## Options considered

1. **Faith as events.** Every change records a `FaithChanged` event. Full provenance, but a completed goal would write thirty rows, flood the bounded consequence views and the feed, and change history in no observable way the player needs.
2. **Faith as a fixed rule on existing events (chosen).** Faith is core state changed by a published table of rules keyed on events the powers already record. Provenance is the triggering event plus the rule; history gains only one goal-completion event.
3. **Faith in Unity.** Rejected: Rust owns authoritative state, and faith must survive a save and replay identically.

For goals, generating them as events at each day start was rejected: an untouched world would gain events every midnight and every existing golden would change. Goals are instead a pure function of the seed, the day and state at the day boundary, and only a completion records an event.

## Decision

**1. Faith.** Each retained resident holds `faith: i16` in −100..=100, starting at 0, clamped after every change. Faith is independent of LOD and of life state (a dead resident keeps their last value; nothing in the slice kills, per ADR 0038). After each committed tick, a post-pass visits that tick's newly recorded events in ascending event ID and applies:

| Event | Rule |
|---|---|
| 14 `BlessApplied { resident, affected }` | resident +15 (whether or not `affected`) |
| 13 `SliceIllnessApplied { resident, affected }` | resident −20 when `affected = 1`; no change otherwise |
| 17 `StayedInside { resident }` | resident −5 per row |
| 21 `JoinedGathering { resident }` | resident +10 |
| 23 `GoalCompleted` (§2) | every retained resident +10 |

The block faith meter is the arithmetic mean of all retained residents' faith, truncated toward zero, as an `i32`. Faith changes no behaviour in this ADR.

**2. Daily goals.** The goal day is the model day counted from ADR 0027's schedule epoch: `day = floor((now − schedule_epoch) / 86 400 s)`, the same boundary the HUD's "Today" schedule uses. The goal set for a day is generated at the first committed tick whose time falls in that day (and at creation and at load for the current day) from the counter-based `ws_rng::Stream::new(seed, Purpose::new("ws-sim/slice/goals"), &[day as u64])`, draws numbered from 0, drawing in this order:

1. `count = 1 + r₀ mod 3` (1 to 3 goals).
2. A permutation of the four templates by successive draws; take the first `count` that are feasible.
3. One target draw per chosen template.

| Kind | Template | Target draw | Feasibility cap from state at the day boundary | Progress |
|---|---|---|---|---|
| 1 | Bring N people to a gathering | 3 + r mod 3 (3–5) | max over featured residential homes of residents within 150 000 mm (ADR 0038 §4 integer rule); target = min(draw, cap); infeasible when cap = 0 | largest `joined` of any single gathering this day |
| 2 | Bless N different people | 2 + r mod 2 (2–3) | living residents; target = min(draw, count) | distinct residents in `BlessApplied` this day (at most 3 IDs kept) |
| 3 | Cure someone with a blessing | 1 | always feasible | `Recovered` (12) events linked `CausedBy` a `BlessApplied` this day |
| 4 | Keep N people inside during a storm | 3 + r mod 4 (3–6) | residents ÷ 5, at least 1; target = min(draw, cap) | largest `StormKeptInside.count` of any storm this day |

The post-pass first checks the day: when the tick's time is in a new day it replaces the goal set before applying any rule, so that tick's events count toward the new day. Progress counts only events recorded in that day. A goal completes at the post-pass that first sees progress ≥ target: it is marked done, records one `GoalCompleted { day, slot, kind, target }` (tag 23, `Origin::System`, linked `ContributedTo` the triggering event, excluded from every consequence index through ADR 0038's producer exclusion table), and then the §1 rule for 23 applies. Goals still open when the day ends expire silently: state only, no event, no faith change. Every goal requires a cast to progress, so a world where no power is used records no new event and no faith change.

**3. Events.** `Payload` gains tag 23 `GoalCompleted { day: i64, slot: u8, kind: u8, target: u8 }` after 22; system 87, importance as ADR 0038's root-adjacent notices. The 80-byte recent-event projection words are day, slot, kind, target, 0. Building and resident inspection never show it (it has no actor), so neither validator changes.

**4. Persistence and determinism.** Save format 6 appends `faith` to each resident's slice row and adds one `slice_goals` table: day, count, and per slot kind, target, progress, done, plus up to three blessed resident IDs for kind 2. The loader reads formats 4, 5 and 6; writers write 6. Formats 4 and 5 load with faith 0 and the current day's goals generated as at a day boundary. The canonical state hash appends a `god` block only when any faith is nonzero or any goal has progress or is done; an untouched goal set is a pure function of hashed state and the day, so existing goldens stay byte-identical when no power is used. New goldens cover each goal kind's completion from a fixed seed, replayed in the same and in a separate process; one test saves mid-day with partial progress and asserts the reload continues identically.

**5. FFI.** One read-only export:

```c
WsStatus ws_slice_god(WsSliceHandle *handle, WsSliceGod *output);
```

`WsSliceGod` (560 bytes, `struct_size` checked): `struct_size u32`, `reserved u32 = 0`, `day i64`, `block_faith i32`, `goal_count u32`, then three goal rows of `kind, target, progress, done` (`u32` each, zero beyond `goal_count`), then `resident_count u32`, `reserved u32 = 0`, then thirty rows of `id u64, faith i32, reserved u32` in ascending resident ID (zero beyond `resident_count`). The snapshot and inspection formats are unchanged. `ABI_VERSION` advances 9 → 10; v9 fixtures and the v9 smoke test are kept and v10 ones added.

**6. Unity.** `NativeSlice.God()` decodes and validates the struct; `SliceHost` reads it at most once a second. The HUD shows a faith meter in the top strip ("Faith +12"), the day's goals with progress ("Bless 2 different people — 1/2", ticked when done), and faith on the resident card as a word: devoted (≥ 40), trusting (≥ 10), unsure (> −10), wary (> −40), fearful. A goal that becomes done adds one feed line ("Goal complete: …"). Presentation only.

## Consequences

- Easier: later gameplay (faith-driven behaviour, goal rewards, a goal history) reads one core state block and one export.
- Harder: format 6 touches `ws-persist` codecs and round-trip tests again; a new post-pass runs after every committed tick (bounded by that tick's events); the ABI advances to 10.
- Affected: `ws-events` (tag 23), `ws-sim` (faith, goals, post-pass, exclusion), `ws-persist` (format 6), `ws-ffi` (`ws_slice_god`, ABI 10, fixtures, smoke test), Unity `Bridge` and `UI`, EditMode tests, `docs/architecture/13-god-interventions.md`, the ADR index.
- Revisit if faith should change behaviour (attendance, obedience), if goals should carry rewards beyond faith, or if a goal that needs no cast is wanted (that would need goldens to change).

## Decisions taken under the standing delegation

1. Faith changes are a rule on existing events, not events of their own, to keep views and the feed readable.
2. Only goal completion is an event; generation and expiry are state only, so an untouched world's history and goldens do not change.
3. Every goal needs a cast; passive goals ("keep everyone healthy") would complete with no player action.
4. Expired goals carry no penalty, so a day without play costs nothing.
5. Save format 6 and ABI 10 are accepted for the new state and its read.

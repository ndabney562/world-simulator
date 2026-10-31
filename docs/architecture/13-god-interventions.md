# 14. The Generalized "God Intervention" System

Covers point 14.

## 14.1 Goal

The player can intervene in the world at any scale, from nudging one person to reshaping a planet, through one general system rather than a pile of special cases. Interventions must work whether their target is currently materialized or not, must remain consistent as the target changes LOD level, and must be saveable, replayable, and (where the design wants it) undoable.

## 14.2 Anatomy of an intervention

An intervention is **data**, submitted as a command ([04](04-simulation-architecture.md)):

| Field | Meaning | Examples |
|---|---|---|
| **Target selector** | What it applies to, resolved deterministically at apply time | One entity; all residents of a building; a circular area; a district; a settlement; a body; "everyone with trait X in region Y" |
| **Effect list** | One or more effects from the effect vocabulary, with parameters | `set_property(wealth, +50%)`, `spawn(person, template)`, `environment(precipitation, heavy, 2h)`, `event(fire, intensity)` |
| **Timing** | When and how long | Now; at time T; over 3 days; permanent |
| **Scale declaration** | Which LOD levels the effects are defined at | L0–L4 for a plague; L6–L7 for an asteroid impact |
| **Provenance** | Who or what issued it | Player, scenario script, (later) an LLM translation the player confirmed |

Interventions are validated (is the target valid, are the parameters in range), applied at a tick boundary, and recorded in the journal.

## 14.3 The effect vocabulary

A finite, versioned set of **effect types**, each a small piece of code with defined semantics at every LOD level it supports. Interventions are compositions of effects. Initial families:

| Family | Effects (illustrative) |
|---|---|
| Existence | Spawn entity from template; remove entity; clone; move/teleport |
| Properties | Set or modify a component value (health, wealth, mood, skill, trait, relationship) |
| Environment fields | Modify weather, temperature, fertility, pollution, disease prevalence over an area and duration |
| Events | Start a typed event (fire, flood, earthquake, festival, epidemic, discovery) with parameters |
| Social / cultural | Introduce an idea, rumour, law, religion, or technology into a group with a strength |
| Structures | Build, demolish, damage, repair a structure; change land use |
| Celestial | Modify orbital elements or physical parameters of a body; spawn an impactor |
| Time | Not an effect on the world: player time controls are separate commands |

Every effect type must implement, for each level it claims to support:
1. **Apply to individuals** (if materialized at L0–L2).
2. **Apply to aggregates** (L3+): the statistical equivalent (for example "infect 2% of district residents" becomes a change to the epidemic compartment model).
3. **Persistence rule**: what it writes into canonical state so the effect survives dematerialization and appears correctly on later materialization.
4. **Invariants** it must respect (conservation, valid ranges).

An effect type that cannot define an aggregate equivalent is restricted to materialized targets (the UI must not offer it for coarse targets). This is the honest constraint from [01](01-hard-problems.md): the vocabulary is extensible, but every addition must define its behaviour at each scale.

## 14.4 How an intervention reaches unmaterialized targets

Example: "plague in New York City" while the city is at L5 (no individuals exist).
1. Apply at L5: the city's epidemic model gets new infected counts and parameters.
2. Flows carry it to neighbouring settlements at L5 through the trade and migration network.
3. The player zooms into a district: disaggregation produces individuals, and the district's epidemic state determines which ones are sick, by seeded sampling. Those individuals now carry an `Infected` component, consistent with the aggregate.
4. Zooming out again folds individual outcomes back into the district's compartments.

The intervention itself is never "replayed" on materialization; its effects already live in canonical state at whatever level it touched, and disaggregation carries them down.

## 14.5 Large and extreme interventions

Some interventions exceed what any model can simulate credibly (destroy the moon, double the sun's brightness, end gravity on a continent). Policy:
- Support them through **parametric outcome models** (scientifically inspired formulas and lookup tables: impact energy to crater size, ejecta, climate cooling, tides) and **scripted consequence chains** (a destroyed moon becomes a debris ring plus scheduled meteor events plus tide changes).
- Be explicit in the design that these are approximations. The simulation's job is to be coherent and dramatic, not to predict reality.
- Some interventions may be ruled out entirely if their consequences cannot be made coherent (for example "turn off gravity" breaks almost every model). That is a design decision, but the architecture must allow saying "no".

## 14.6 Undo, branching, and replay

Because the simulation is replay-exact on the same build and platform (Tier 2, [25](25-determinism-policy.md)) and every intervention is a journaled command:
- **Undo** = restore the nearest snapshot before the intervention and replay the journal without it. Snapshots are taken periodically and before every intervention, so undo is fast.
- **Branching timelines** (try an intervention, compare outcomes) are the same mechanism and can be added later at little architectural cost.
- **Replay** of interventions is exact, which makes interventions testable as scenarios.

Whether undo is a player-facing feature is a design question; the architecture supports it either way.

## 14.7 Player-facing layers (later)

1. **Direct tools** for common interventions (tap a person, choose "bless with wealth").
2. **Intervention composer** that lets the player assemble target + effects + timing.
3. **Natural-language input** (optional, LLM-based): free text is translated into a candidate intervention in the effect vocabulary, shown to the player for confirmation, then submitted like any other. The LLM never applies effects directly ([16](16-ai-llm-npcs.md)).

## 14.8 Causal provenance of interventions

Every applied intervention creates an `player.intervention_applied` **world event**, the root of its causal tree ([24](24-world-events-and-causality.md)). From there, consequences are linked by the practical provenance model in [24 §24.6](24-world-events-and-causality.md):

1. **Direct effects** of each effect type are emitted as events with `caused_by` links to the root. For example, "curse with illness" causes an `life.illness` event for the target.
2. **Provenance tags** are attached to every component, field, or aggregate the effects changed. Later events produced while reading tagged state record the intervention as a `contributed_to` cause, with weights that decay with each hop and with time. Intervention roots are always tracked; tags are bounded per piece of state.
3. **Aggregate attribution:** effects applied at L3+ keep an attributable share in the coarse models (infections descending from the plague's seed cases; the economic delta from a "bless the shop" effect), so aggregate events can state an attributed fraction.
4. **Optional counterfactual summary:** for a "what did this do?" screen, the game may run a background, coarse-level branch from the pre-intervention snapshot without the intervention and compare the outcomes. This is possible because the simulation is replay-exact on the same build ([25](25-determinism-policy.md)).

Each linked consequence carries a confidence class (`direct`, `contributing`, `statistical`, `counterfactual`), and the UI words them differently ("caused", "contributed to", "associated with").

Requirements on every effect type (adding to 14.3):
- Declare which event types it emits directly.
- Declare which state it tags.
- At aggregate levels, declare how its attributable share is tracked, or state that it is untracked (in which case explanations fall back to counterfactuals or say "unknown").

## 14.9 Observability

Every intervention has an inspector record: what it targeted, what it changed at each level, and a timeline of consequences built from the causal links and attributions above. Players and developers can answer "what did my plague actually do?" and "what happened here?", and see how confident each answer is.

## 14.10 First implementation (TP-010)

[ADR 0021](../adr/0021-god-intervention-contract.md) records the first intervention: the `Intervene` command (target, effect, timing; provenance is the journal entry), the one-effect vocabulary `afflict(illness, severity)` at L1 and L2, the `intervention.applied` / `life.illness` / `agent.returned_home` events and links of §14.8, and `why_ill`. Aggregate application, other effects and the FFI are later work.

## Accepted real-slice illness increment

[ADR0034](../adr/0034-slice-illness-and-consequences.md) accepts #74’s finite fever operation, continuous home/rest/rejoin overrides, actual missed duty, L1 apartment-neighbour check-in and recovery. Native consequences preserve direct versus contributing claims; player details remain five fields. Runtime follows this separately reviewed and accepted design.

## Accepted reversible powers increment

[ADR0038](../adr/0038-bless-storm-gathering-powers.md) accepts #125's three reversible powers: bless (early recovery or relieved needs), a one-hour storm that defers baseline departures, and a gathering that brings up to seven neighbours to one building under a new override kind. Each is a slice operation with a root event, bounded consequences that rebuild identically on load, and save format 5. No death, no new UI mechanism.

### Implemented in GP-002 (#125, branch `claude/125-god-powers-impl`)

The three powers are implemented end to end: event payloads 14 to 22, `ws-life` storm holds and routine kind 3, `ws-sim` episodes, save format 5, ABI 9 and the Unity HUD. Decisions made while implementing, in addition to ADR 0038:

- **D1, storm hold state.** Saved resident state has no `logical` field (it is recomputed from the routine or baseline timetable), so a storm-deferred resident needs saved state. `ResidentStateSnapshot` gains `storm_hold: Option<StormHold>`, saved in each format-5 `slice_behavior` row and hashed only when `Some`, so existing goldens are unchanged. It is not a routine override. As amended in the final review, inspection publishes it as override kind 4 with the hold's storm `root` (saved and hashed with the hold) as the override root; ADR 0038 §5 has the rule.
- **D2, gathering distance.** The block dataset has no integer-millimetre positions; `SourceBuilding.centroid` is a `GeoPoint` in pico-degrees. Distance uses a fixed integer equirectangular conversion: `dy_mm = Δlat * 111_133 / 10^9`, `dx_mm = Δlon * 84_383 / 10^9`, in `i128` (mm per micro-degree at latitude 40.622 degrees). Eligible when `dx² + dy² <= 150_000²`. A golden test pins the six pairwise distances between the four featured buildings.
- **D3, post-storm resume (revised after the Task 3 review).** At the first re-evaluation at or after `storm.end`, a held resident whose baseline is still inside the deferred row's walk window departs now with the row's own duration. Otherwise they walk (route walking time) from the held building to where the baseline places them now, then rejoin from there, or rejoin at once if already there. The first rule (hold until rejoin) stranded residents for hours. The hold stores its phase (deferring or resuming) explicitly and never derives it from the storm window.
- **D4, producer system numbers.** Roots use system 10; bless effects 84, storm effects 85, gathering effects 86; `ReturnedHome` keeps 50. Roots have importance 32768; other new tags reuse the level of their nearest ADR 0034 analogue.

Behaviour worth knowing:

- **Bless cure.** The illness routine is recomputed as natural recovery would be. The illness group's `end` stays the originally scheduled end, and the illness group gets no `Recovered` row: the bless group owns it.
- **Save format 5** persists the slice storm and a separate `ws-life` storm window. The window outlives the storm episode until every hold rejoins. It also persists the gathering with its `joined` counter and each attendee's phase, and each resident's storm hold, phase included. Format 4 still loads, with no episodes and no holds.
- **ABI 9** adds the read-only export `ws_slice_last_root(handle, uint64_t*)`, returning the root of the most recently accepted intervention (operations 5 to 8) or 0; it exists because a storm root has no actor. The consequence header subject is 0 only for a storm root. The native inspection encoder refuses incoherent override state, and override kinds 3 and 4 are the only kinds allowed away from home. The ABI 8 fixtures and `abi-v8-smoke.c` are kept; v9 fixtures and `abi-v9-smoke.c` are added.
- **HUD.** Bless and Gathering are select-then-tap, Storm casts for one hour, and a Gathering from the HUD lasts 30 minutes.


## Accepted faith and daily goals increment

[ADR0039](../adr/0039-faith-and-daily-goals.md) accepts #127's resident faith and daily goals. Each resident holds faith from -100 to 100 (bless +15, an affecting curse -20, staying inside -5, joining a gathering +10, each completed goal +10 to everyone); the block faith meter is the truncated mean. Each model day generates one to three goals from the seed, the day and state at the boundary; every goal needs a cast, and only a completion records an event. Faith is a rule on existing events, not events of its own.

### Implemented in GP-004 (#127, branch `claude/127-faith-goals`)

Faith and goals are implemented end to end: event payload 23 `GoalCompleted`, `ws-sim` owned `God` state and a per-commit `god_pass`, save format 6, ABI 10 with the read-only `ws_slice_god`, and the Unity faith meter, goal list and card faith word. Rulings made while implementing, in addition to ADR 0039 (recorded as its marked amendment):

- **D1, faith storage.** Faith is saved in its own `slice_faith` table (`resident_id`, `faith`), not appended to the resident row. The `slice_goals` blob also stores the current gathering (building, joined).
- **R1, gathering progress.** Progress is the largest count of `JoinedGathering` rows sharing one building recorded today; the core keeps the current gathering's (building, joined) and saves it.
- **R2, bless goal.** "Bless N different people" is skipped, like an unreachable gathering, when no resident is living, so no goal completes without a cast.
- **Event sequence.** `GoalCompleted`'s sequence is its position among the batch's completions; the slot is in the payload. It is linked `ContributedTo/Direct` the triggering candidate and never indexed in consequences.
- **Old saves.** Formats 4 and 5 load with faith 0 and freshly generated goals for the restored day. The state hash mixes the god block only once any faith is nonzero or any goal has progress, so worlds that use no power keep their goldens.

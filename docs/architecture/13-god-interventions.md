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

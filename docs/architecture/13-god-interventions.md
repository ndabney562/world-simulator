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

Example: "plague in this city" while the city is at L5 (no individuals exist).
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

Because the world is deterministic and every intervention is a journaled command:
- **Undo** = restore the nearest snapshot before the intervention and replay the journal without it. Snapshots are taken periodically and before every intervention, so undo is fast.
- **Branching timelines** (try an intervention, compare outcomes) are the same mechanism and can be added later at little architectural cost.
- **Replay** of interventions is exact, which makes interventions testable as scenarios.

Whether undo is a player-facing feature is a design question; the architecture supports it either way.

## 14.7 Player-facing layers (later)

1. **Direct tools** for common interventions (tap a person, choose "bless with wealth").
2. **Intervention composer** that lets the player assemble target + effects + timing.
3. **Natural-language input** (optional, LLM-based): free text is translated into a candidate intervention in the effect vocabulary, shown to the player for confirmation, then submitted like any other. The LLM never applies effects directly ([16](16-ai-llm-npcs.md)).

## 14.8 Observability

Every intervention gets an inspector record: what it targeted, what it changed at each level, and a timeline of consequences (derived from the history log by causal tagging: events carry the ID of the intervention that caused them, directly or through a chain). Players and developers can answer "what did my plague actually do?"

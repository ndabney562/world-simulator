# 17. AI / LLM Augmentation of NPCs (without depending on it)

Covers point 17.

## 17.1 Principle

**The simulation must be complete, reproducible ([25](25-determinism-policy.md)), and fully playable offline with no LLM at all.** LLMs are an optional layer that can make the world more expressive. They never sit on the causal path of canonical state unless their output has been converted into an ordinary, validated, journaled command.

Reasons:
- Reproducibility and replay: an LLM call is non-deterministic and model versions change ([25](25-determinism-policy.md), Tier 4).
- Latency: seconds per call, versus milliseconds per tick.
- Cost and availability: cloud calls cost money and need a network; on-device models consume battery and memory.
- Scale: thousands of agents per minute is far beyond any sensible call budget.
- Safety and coherence: free-form model output can contradict world facts.

## 17.2 Where LLMs can help (in order of value and safety)

| Use | How it works | Touches canonical state? |
|---|---|---|
| **Dialogue surface text** | The deterministic NPC mind decides *what* to communicate (intent, facts, emotion); the LLM turns that structured content into natural text in the NPC's voice. Template text is the fallback. | No |
| **Descriptive flavour** | Biographies, diary entries, local news headlines, epitaphs, neighbourhood descriptions, generated from structured world events ([24](24-world-events-and-causality.md)). | No (text may be cached as presentation data) |
| **Inspector explanations** | "Why is this person unhappy?", "What happened here?", and "What did my intervention do?" phrased from the structured answers the event and causality system already computes, including confidence classes. | No |
| **Natural-language god commands** | Player free text is translated into a candidate intervention in the effect vocabulary ([13](13-god-interventions.md)), shown for confirmation, then submitted as a normal command. | Only via the validated, journaled command |
| **Notable-NPC suggestions** (later, experimental) | For a few notable NPCs, the LLM proposes a goal or decision from a constrained menu, with reasons. The simulation validates it and may adopt it; adopted choices are recorded in the journal as inputs. | Only via a validated, journaled command |
| **Developer tooling** | Generating name lists, culture flavour tables, building-style grammars, and test scenarios offline, reviewed and committed as data. | Only as reviewed content |

## 17.3 The adoption pattern (for anything that changes state)

```
structured state ─► prompt builder ─► LLM ─► schema-constrained output
                                                   │
                                     validator (types, ranges, world facts)
                                                   │ accepted
                                                   ▼
                                    ordinary command ─► journal ─► simulation
```

Key property: **the journal records the adopted command, not the prompt.** Replaying a save never calls the LLM; it replays the recorded decision. The command's resulting world events carry provenance `origin: adopted LLM suggestion`, so explanations can say so.

## 17.4 Context and memory

LLMs do not hold NPC memory. NPC memory is structured data in the core: references to world events, relationships, and personal facts ([09](09-entities-and-agents.md), [24](24-world-events-and-causality.md)). A prompt builder retrieves the relevant events and facts for a request and serialises them compactly, including causal links and their confidence. This keeps memory consistent, inspectable, testable, and model-independent, and it means the same event system feeds memories, history, statistics, explanations, and dialogue.

## 17.5 Providers

- **On-device:** Apple's on-device foundation model framework (available on Apple Intelligence-capable devices, which overlap with our proposed minimum device) offers free, private, offline generation suited to short dialogue lines and descriptions with structured (guided) output. Quality and context size are limited.
- **Cloud:** larger models (Claude or others) for richer text and harder translation tasks (natural-language interventions). Requires network, user consent, and a cost budget; must be optional.
- The core defines a provider-neutral interface (request type, structured response schema, timeout, fallback); the Unity front end implements providers (Apple's on-device model needs a small native Swift plugin). Provider choice is deliberately postponed ([21](21-decisions-now-vs-later.md)).
- Zero-cost policy (ADR 0007): the on-device model is free; cloud models cost money, so they are opt-in at most and never required.

## 17.6 Practical rules

- Every LLM feature has a deterministic fallback (templates) and a timeout; the game never waits on an LLM to advance the simulation.
- Calls are asynchronous and budgeted (per minute, per session); results are cached by content hash.
- Output is validated against world facts (names, places, relationships) before display; contradictions are rejected and the fallback used.
- Player-visible content generation follows platform content policies; prompts include safety constraints.
- No LLM feature is in the Technical Prototype, Vertical Slice, or Neighbourhood MVP ([20](20-mvp.md)). The first LLM feature, if any, should be dialogue surface text for inspected notables, because it is presentation-only and easy to turn off.

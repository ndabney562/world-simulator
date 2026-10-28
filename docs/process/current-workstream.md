# Current workstream

Updated 2026-10-28 for [#112](https://github.com/ndabney562/world-simulator/issues/112). GitHub issues/PRs remain canonical; refresh their live state at each handoff.

## Authorization and ownership

Noah's 2026-10-19 and 2026-10-20 instructions recorded on [#77](https://github.com/ndabney562/world-simulator/issues/77) authorize the current technical backlog (#13, #30, #49, #53, #66–#76), dependent starts and its technical merges after latest-head green CI and a clean independent actual-diff review. The specifically recorded design acceptances retain their own scope; this is not blanket acceptance of future ADRs. Noah directly authorized the five-step improvement plan and #104 implementation on 2026-10-26. #104 is outside the earlier named merge delegation and requires Noah's merge approval.

Codex is the implementer for the resumed plan. A different assistant reviews the actual published diff. One writer owns each branch. The implementer remains responsible through fixes, CI, readiness, delegated merge and the next dependency handoff; successful monitor recovery does not finish that handoff.

Costs, new credentials/accounts, repository settings, test weakening, Apple signing/provisioning/device actions and stage-exit declarations remain separate gates. Scope or architecture changes not covered by a specific recorded acceptance still require Noah's decision. Reversible work within authorized scope proceeds without repeated permission requests.

## Current state and next action

| Work | Engineering state | Remaining acceptance / next owner |
|---|---|---|
| Technical Prototype | Complete; optional provider spike #13 remains separate | Do not block core work on optional providers |
| VS-001–007 (#65–#71) | Merged, issues closed | Preserve existing invariants and regression tests |
| Rendering #72 / inspection #73 | Merged in #95 / #98 | Noah: authorized iPhone build/run, screenshots, data comparison and device performance |
| Illness #74 | #103 merged at `e5276e347fc68ab18fad8497055f00e7ea50f343`; reviewed head `8269e5bb9d96cb31eeb02fb21e8e4d4c260132f0`, all five checks passed | Completed; final device coverage remains #76 |
| Persistence #53 / #75 | Merged in #108 / #109; format 4 under accepted ADR0036 | Preserve native save/load coverage; browser durability is separate |
| Browser #106 / #107 | Spike #111 and accepted ADR0035 (acceptance record PR #113) #110 merged | #112: pinned complete-player build and desktop release preparation; phone browsers later |
| Final run #76 | Open | Noah: iPhone evidence and stage declaration |

#53 delivered baseline/delta storage; #75 delivered slice state, background saving and Unity controls under accepted ADR0036. Do not repeat completed persistence work. #76 records every criterion as met, explicitly waived by Noah, or open with a follow-up. Neither engineering integration nor a desktop benchmark closes device criteria.

## Handoff rule

Every pause identifies the issue, owner, branch/head, actual checks, independent review, remaining acceptance and one next action. A draft PR with clean review and green checks must be marked ready and merged by its authorized owner, or name the specific remaining gate. Keep #77's checkboxes aligned with completed acceptance, with separate engineering/device status where needed.

## Desktop browser release preparation (#112)

Noah authorized the plan directly on 2026-10-28 in the local desktop checkout and selected desktop browsers first, phone browsers later. This authorization is recorded in #112. Codex owns branch `codex/112-desktop-browser`; one writer per branch. Keep compatibility/build experiments separate from the accepted browser approach in ADR0035. Noah explicitly approved the browser approach and MIT for his code on 2026-10-28; the acceptance record is PR #113. Technical merges for #112 and #117 are delegated after final-head green CI, clean independent actual-diff review and matching local Apple Silicon native evidence ([recorded delegation](https://github.com/ndabney562/world-simulator/issues/112#issuecomment-6479471174)). Publication/settings remain separate explicit decisions.

Local candidate in PR #114 now has a pinned full-player export, durable reload/import verification in desktop Chromium, save-integrity repairs #115/#116, browser fault-boundary tests, complete payload measurement and collected notices. See [verification](../verification/issue-112/README.md). The runner fallback (#117 / PR #118) is merged at `111b5acd8376438dcb7c9e72cbdc976cce5f2694`; current browser branches integrate its free hosted matrix and mandatory local Apple Silicon policy. Remaining release gates: final independent review and hosted CI, wider desktop browser/clean-device and full-player fault/storage-denial checks, and explicit publication approval. #13 optional providers block none of this. Native iPhone acceptance remains separately tracked in #72/#73/#76.

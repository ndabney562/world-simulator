# Current workstream

Updated 2026-10-26 for [#104](https://github.com/ndabney562/world-simulator/issues/104). GitHub issues/PRs remain canonical; refresh their live state at each handoff.

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
| Illness #74 | #103 merged at `e5276e347fc68ab18fad8497055f00e7ea50f343`; reviewed head `8269e5bb9d96cb31eeb02fb21e8e4d4c260132f0`, all five checks passed | Codex: continue #53 then #75; final device coverage remains #76 |
| Persistence #53 → #75 → final run #76 | Open | Codex: implement accepted ADR0025 storage before whole-slice integration; Noah: final device evidence and stage declaration |

#53 owns baseline/delta storage; #75 owns all slice state, background saving and Unity controls. Accepted ADR0025 exists; do not repeat its design phase. #75 still needs the required reviewed amendment/version contract for added slice layouts. #76 records every criterion as met, explicitly waived by Noah, or open with a follow-up. Neither engineering integration nor a desktop benchmark closes device criteria.

## Handoff rule

Every pause identifies the issue, owner, branch/head, actual checks, independent review, remaining acceptance and one next action. A draft PR with clean review and green checks must be marked ready and merged by its authorized owner, or name the specific remaining gate. Keep #77's checkboxes aligned with completed acceptance, with separate engineering/device status where needed.

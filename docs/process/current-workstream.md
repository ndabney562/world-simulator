# Current workstream

Updated 2026-10-30 for [#112](https://github.com/ndabney562/world-simulator/issues/112). GitHub issues/PRs remain canonical; refresh their live state at each handoff.

## Authorization and ownership

Noah's 2026-10-19 and 2026-10-20 instructions recorded on [#77](https://github.com/ndabney562/world-simulator/issues/77) authorize the current technical backlog (#13, #30, #49, #53, #66–#76), dependent starts and its technical merges after latest-head green CI and a clean independent actual-diff review. The specifically recorded design acceptances retain their own scope; this is not blanket acceptance of future ADRs. Noah directly authorized the five-step improvement plan and #104 implementation on 2026-10-26. From 2026-10-29 the [standing delegation](human-approval-gates.md#standing-delegation-2026-10-29) (#122) covers all work, including #104.

Codex is the implementer for the resumed plan. A different assistant reviews the actual published diff, or a fresh-context review agent when none is available. One writer owns each branch. The implementer remains responsible through fixes, CI, readiness, delegated merge and the next dependency handoff; successful monitor recovery does not finish that handoff.

Costs, new credentials/accounts, repository settings, test weakening, Apple signing/provisioning/device actions and stage-exit declarations remain separate gates. Scope or architecture changes not covered by a specific recorded acceptance still require Noah's decision. Reversible work within authorized scope proceeds without repeated permission requests.

## Current state and next action

| Work | Engineering state | Remaining acceptance / next owner |
|---|---|---|
| Technical Prototype | Complete; optional provider spike #13 remains separate | Do not block core work on optional providers |
| VS-001–007 (#65–#71) | Merged, issues closed | Preserve existing invariants and regression tests |
| Rendering #72 / inspection #73 | Merged in #95 / #98 | Noah: authorized iPhone build/run, screenshots, data comparison and device performance |
| Illness #74 | #103 merged at `e5276e347fc68ab18fad8497055f00e7ea50f343`; reviewed head `8269e5bb9d96cb31eeb02fb21e8e4d4c260132f0`, all five checks passed | Completed; final device coverage remains #76 |
| Persistence #53 / #75 | Merged in #108 / #109; format 4 under accepted ADR0036 | Preserve native save/load coverage; browser durability is separate |
| Desktop browser #112 | Player #114, runner fallback #118 and release checks #119 merged; verified candidate published from `gh-pages` | [Play desktop preview](https://ndabney562.github.io/world-simulator/); phone/other-device evidence remains separate |
| God powers GP-002 #125 | ADR 0038 accepted; bless, storm and gathering implemented on `claude/125-god-powers-impl` (save format 5, ABI 9, HUD); decisions D1 to D4 in [architecture 13](../architecture/13-god-interventions.md) | Merged in #131 |
| Visible reactions GP-003 #126 | Cast effects, status icons, camera ease and follow on `claude/126-visible-reactions` (Unity only) | Review, CI, merge under the standing delegation |
| Final run #76 | Open | Noah: iPhone evidence and stage declaration |

#53 delivered baseline/delta storage; #75 delivered slice state, background saving and Unity controls under accepted ADR0036. Do not repeat completed persistence work. #76 records every criterion as met, explicitly waived by Noah, or open with a follow-up. Neither engineering integration nor a desktop benchmark closes device criteria.

## Handoff rule

Every pause identifies the issue, owner, branch/head, actual checks, independent review, remaining acceptance and one next action. A draft PR with clean review and green checks must be marked ready and merged by its authorized owner, or name the specific remaining gate. Keep #77's checkboxes aligned with completed acceptance, with separate engineering/device status where needed.

## Desktop browser publication (#112)

Noah authorized the plan directly on 2026-10-28 in the local desktop checkout and selected desktop browsers first, phone browsers later. This authorization is recorded in #112. The implementation and release-check branches are merged; one writer per active follow-up branch. Keep compatibility/build experiments separate from the accepted browser approach in ADR0035. Noah explicitly approved the browser approach and MIT for his code on 2026-10-28; the acceptance record is PR #113. Technical merges for #112 and #117 are delegated after final-head green CI, clean independent actual-diff review and matching local Apple Silicon native evidence ([recorded delegation](https://github.com/ndabney562/world-simulator/issues/112#issuecomment-6479471174)). Noah approved the GitHub Pages publication on 2026-10-28 and reconfirmed that approval on 2026-10-30. Publication and the specific Pages setting are approved and performed; other settings/cost/device/stage gates remain separate.

The complete player is published at [the desktop preview](https://ndabney562.github.io/world-simulator/) from `gh-pages` commit `bf1493399c5149183757646c2b4ce0bd1f70c83b`, with HTTPS enforced. Runtime candidate `8de9a2cb` remains byte-identical to the reviewed nine-file manifest; #119 added release tools/evidence without shipping-code changes. Three-engine fresh-profile storage/import/recovery and headless GPU measurements plus manual Safari visual/panic recovery are recorded in [release checks](../verification/issue-112/release-checks.md). Public HTTPS delivery, the deployed manifest and all nine player hashes were rechecked on 2026-10-30; see the [publication record](../verification/issue-112/publication-proposal.md). Noah reported a public-origin smoke test on 2026-10-30 in his desktop browser at [the live preview](https://ndabney562.github.io/world-simulator/): saving worked, and the saved world loaded after a page reload. Backup export/import on the live site and other devices were not tested in this smoke test; the release checks retain separate prepublication export/import evidence. No fault-probe files, profiles, saves or raw logs were deployed. Another physical desktop/Windows/Linux, Intel/low-memory graphics, phones/offline and native iPhone acceptance remain explicitly unverified; no device/stage exit is claimed.

# Handoff

- **Goal:** A playable god simulator with a fun interface ([#122](https://github.com/ndabney562/world-simulator/issues/122)). Assistants open issues, implement, review and merge under the [standing delegation](docs/process/human-approval-gates.md#standing-delegation-2026-10-29).
- **Done:** 2026-11-05: PROJECT.md and HANDOFF.md moved into the repo (they were local-only, so the weekly health score could not read them); the merge rule now states the standing delegation. 2026-11-01: toy world deleted (#147, #148, #149). 2026-10-31: GP-002 to GP-004 merged (#131, #135, #137), Vertical Slice exit report (#145), process change to one PR per feature (#146).
- **In progress:** none.
- **Next step (exact):** [Assigned: Claude Code] Pick the smallest open follow-up (#132 HUD copy for rejected curse and inspection failures), branch `claude/132-hud-copy` from main, fix it, run the test command in PROJECT.md, open a PR and merge it once CI is green and the review is clean.
- **Blockers:** none for engineering. Noah's gates: on-device iPhone evidence and the Vertical Slice stage exit (#76).
- **Last test result:** `python3 scripts/check_docs.py` pass (2026-11-05, docs-only change).
- **Last commit hash:** 2fc27f9 (main before this change).
- **Model + timestamp:** Claude Code (Opus 5.5), 2026-11-05.

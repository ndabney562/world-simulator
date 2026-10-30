# Shared development: Codex and Claude

This is the practical entry point; [AGENTS.md](../../AGENTS.md) and the existing process documents remain the rules. No assistant shares memory with another. GitHub carries context between sessions.

## Daily loop

1. **Assign:** Noah picks an open issue, checks its dependencies, and names the implementer and a different reviewer. Record owner, intended branch, likely files, acceptance criteria, and status in the issue. A GitHub issue alone does not start an assistant: give its URL to that assistant and explicitly authorize the run.
2. **Start:** The implementer reads AGENTS.md, the issue and its comments, relevant accepted ADRs, and any latest handoff. Confirm repository access, working directory, branch, base SHA, and available tools. Fetch current `main`; use a dedicated environment or worktree.
3. **Build:** One writer owns each branch. Keep the issue scope; update tests and docs. For parallel issues, compare file ownership first. Serialize shared API, Cargo manifest/lockfile, scene, ProjectSettings, and common CI/doc changes when they overlap. Agree on interfaces in an accepted ADR where required before dependent code starts.
4. **Publish:** Push the branch, open a draft PR using the existing template, link the issue, and write the handoff below. Record actual checks and unavailable checks separately. Never treat static Unity checks as an Editor run.
5. **Review:** Send the PR URL and head SHA to the other assistant, or run a fresh-context review agent when none is available ([standing delegation](human-approval-gates.md#standing-delegation-2026-10-29)). Ask it to review the actual diff against the issue and ADRs, with findings in the existing severity format. If it cannot post to GitHub, Noah relays its findings. The author fixes findings with new commits and reruns checks; review follows the latest SHA.
6. **Test and merge:** Run required Unity checks on the available Mac and record actual results with the tested SHA; device/account actions retain their separate gates. CI must be green, required independent review resolved, and human gates satisfied. The implementer merges under the standing delegation. The implementer owns readiness and the dependency handoff even after a monitor reports CI recovery. The next authorized task starts from updated `main`; repeat affected verification after conflicts or integration changes.
7. **Pause or hand off:** Leave a checkpoint on the issue or PR even when unfinished. No automatic assistant-to-assistant messaging is assumed. Noah passes the link to the next session.

Use issues and PRs for progress rather than a second status spreadsheet. Architecture decisions remain ADRs through the [architecture change process](architecture-change-process.md); task decisions belong on the issue or PR. Chat decisions must be recorded before another session relies on them. Never put tokens, passwords, signing material, private authorization URLs, or QR codes into these records.

## Separate local checkouts

Cloud environments normally have their own checkouts; confirm this when starting. Two local assistants must never edit the same working directory concurrently, even on different intended branches. Worktrees share Git history but isolate files.

For a fresh Mac checkout (skip cloning if you already have one):

```sh
git clone https://github.com/ndabney562/world-simulator.git
cd world-simulator
git status --short
git fetch origin
```

Example for issue #29, only after Noah assigns and authorizes it:

```sh
git worktree add -b codex/29-nightly ../world-simulator-codex-29 origin/main
cd ../world-simulator-codex-29
```

Use a different branch and directory for Claude's different issue. If a cloud branch already exists, fetch it and create a review worktree instead of recreating it:

```sh
git fetch origin
git worktree add --detach ../world-simulator-review origin/codex/29-nightly
```

A detached review checkout is for review/testing. The branch owner commits changes in its own checkout. Alternatively use separate full clones if an app cannot use worktrees. Never copy a Unity `Library/` directory between checkouts; close the Editor before switching branches or running batch tests. Open `apps/unity/` from the intended checkout in Unity Hub. Coordinate scene and asset edits even with separate worktrees; preserve `.meta` GUIDs. Unity Smart Merge setup is in [the Unity README](../../apps/unity/README.md).

To integrate new `main` while keeping review history:

```sh
git status --short
# Stop if there are uncommitted changes; preserve them before integrating.
git fetch origin
git merge origin/main
# Resolve conflicts deliberately, rerun affected checks, commit and push.
```

Do not force-push a branch under review or overwrite another assistant's branch. Transfer branch ownership explicitly in a handoff if the same task changes implementer.

## Handoff format

Paste into the issue or PR; update at each pause, push, or ownership transfer:

```text
Objective / issue URL:
Implementer / reviewer / status:
Branch / PR URL:
Base SHA / current remote head SHA:
Changes: what changed, why, and important files
Verification: exact commands, results, environment, tested SHA, CI run URLs
Not verified: Unity/device/platform checks still needed and who runs them
Blockers / decisions needed:
Next steps: ordered actions, next owner, acceptance criteria remaining
```

When assigning the next run, use: "Work on ISSUE_URL as implementer/reviewer. Read AGENTS.md, current-workstream.md and the latest handoff. Confirm branch/head and the recorded authorization, stay within scope, and report verification. Follow the standing delegation in human-approval-gates.md; stop only at a gate kept for Noah."

## Phone, Mac, and account access

| Activity | Where |
|---|---|
| Create issues, choose priorities, pass links and prompts, read handoffs | Phone or Mac |
| Inspect PR changes, checks, discussions, and merge after all required evidence | GitHub on phone or Mac; large diffs are easier on Mac |
| Start an assistant coding task | Phone if that assistant's connected cloud coding environment is available to your account; otherwise queue the issue and start it on Mac |
| Change/build/test Rust | Any configured development environment with the pinned toolchain; plain chat and GitHub mobile do not provide a compiler |
| Import scenes/assets, run Editor tests, inspect rendering and Profiler | Mac with recorded Unity version and required modules |
| Build, sign, install, and profile iOS app | Mac with Unity, Xcode, provisioning, and the test iPhone; follow existing human gates |
| Control a Mac-local Codex task from phone | Optional Codex remote access; Mac must be available as required by that feature |

No direct Mac–phone connection is needed for GitHub collaboration: each device needs internet and its own authorized account session. A cloud coding task can run independently of the Mac; a remotely controlled Mac task depends on that Mac. Device deployment/profiling has separate pairing/USB/network requirements. Do not infer that failed Codex remote access means GitHub or iOS device pairing is broken.

Access verified for this setup: Codex's GitHub connection can read and write this repository; Noah reports Claude uses a GitHub-connected environment and there is no unpushed Mac work. This does not prove another chat, a mobile session, or either assistant can access Unity or the Mac filesystem.

Use the existing GitHub connections. For another environment, authorize only this repository and the necessary contents, issues, and pull-request capabilities; changing Actions workflows may require additional workflow permission in that environment. A local clone needs its own GitHub authentication for pushing (SSH or an authenticated Git credential manager). No shared assistant password, new API key, inbound port, or new service is required for this workflow. Keep credentials in the platform's authentication flow or secret store. Repository settings remain Noah's responsibility; verify required checks before relying on protection.

Codex reads [AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md); the existing [CLAUDE.md](../../CLAUDE.md) imports the same file using Claude's [documented import syntax](https://code.claude.com/docs/en/memory). Preserve this single rule source. In plain chat or an environment without automatic instruction loading, explicitly supply/read the repository instructions. [Codex cloud documentation](https://learn.chatgpt.com/docs/cloud) explains the distinction between cloud work and local execution; feature availability must be checked in the actual account.

For optional phone pairing, use the official [Remote connections guide](https://learn.chatgpt.com/docs/remote-connections): update both apps, confirm the same account/workspace, keep the Mac awake and online, restart desktop setup and scan a fresh QR code, then finish any required authentication. A verification screen whose Continue button does nothing does not establish a network failure; its cause remains unconfirmed. If it persists, report app/iOS/macOS versions and the stage that stalls to support, without sharing the QR code or authorization URL.

## Initial assessment (historical)

The first assessment of this repository, taken on 2026-10-16 at `main` commit `0b54a941fdf6ebc4fc69b5fca46ba6e23254ad6e`, proposed a first milestone: Codex implements the nightly LOD workflow (#29), Claude drafts the bridge ADR (S-3 of #4), each reviewed by the other assistant, plus a Noah-run Unity Editor baseline on the Mac. That milestone has since been carried out (see #36, #41, #42). Current status lives in the issues and PRs, starting from [tracking issue #14](https://github.com/ndabney562/world-simulator/issues/14), not in this guide.

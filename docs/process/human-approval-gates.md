# Human Approval Gates

These decisions belong to the Product Owner unless the standing delegation below delegates them. An AI collaborator may propose them, but never acts on them without the Product Owner's explicit approval, written on GitHub (a PR approval, merge, or comment) or given directly to the assigned assistant and then recorded on the relevant issue or PR. The [standing delegation](#standing-delegation-2026-10-29) below delegates some of these gates to assistants.

| Gate | Examples |
|---|---|
| **Merging into `main`** | Delegated by the [standing delegation](#standing-delegation-2026-10-29): assistants merge their own PRs after latest-head green CI and a clean review. |
| **Starting an engineering run** | Delegated by the [standing delegation](#standing-delegation-2026-10-29) for work that serves its priority. |
| **Architecture changes** | Delegated by the [standing delegation](#standing-delegation-2026-10-29): assistants may accept ADRs (superseding one counts as accepting its replacement), recording the reason; Noah can reverse any of them. |
| **Scope changes** | Delegated by the [standing delegation](#standing-delegation-2026-10-29), except declaring a stage complete. |
| **Test and CI weakening** | Deleting, skipping, disabling, or loosening a test or CI check; lowering a performance budget or tolerance; updating Tier 1 golden values. |
| **Cost** | Anything that may cost money: paid services, paid tiers, API usage above free tiers, paid assets or plugins (ADR 0007). |
| **External services and credentials** | Adding a third-party service, API key, secret, or account; changing Google or other provider quotas. |
| **Licences and data** | Adding a data source, asset, or dependency with a licence that is not permissive (MIT, Apache-2.0, BSD, Zlib, or similar), and every new data source (it must be registered in `docs/data/`). |
| **Apple and devices** | Signing, provisioning, TestFlight, App Store, anything using the Apple developer account; on-device runs on the Product Owner's iPhone. |
| **Repository settings** | Default branch, branch protection, rulesets, Actions permissions, collaborators, visibility. |
| **Irreversible actions** | Force-pushing or rewriting `main`, deleting branches with unmerged work, deleting issues or releases. |
| **Stage exit** | Declaring a stage (for example the Technical Prototype) complete. |

## Standing delegation (2026-10-29)

Noah, 2026-10-29 ([#122](https://github.com/ndabney562/world-simulator/issues/122)): "i shouldnt have to review this in the future. I just want a playable god simulator with a fun interface." This delegation applies to all work until Noah narrows or withdraws it.

- **Priority:** a playable god simulator with a fun interface.
- **Work:** assistants open issues, start work and propose or accept ADRs and scope changes that serve that priority, recording the reason on the issue or PR.
- **Review:** a different assistant reviews the actual diff when one is available. When none is (for example usage limits), a fresh-context review agent that sees only the issue and the diff reviews it instead. The PR states which review was used.
- **Merge:** the implementer squash-merges once latest-head CI is green and no Blocker or Major finding is open.
- **Still Noah's:** cost, external services and credentials, licences and new data sources, Apple and device actions, repository settings, irreversible actions, test and CI weakening, and stage exit.
- Noah is not asked to review diffs. Report what merged in one or two plain sentences.

## Recorded delegation

Check [current-workstream.md](current-workstream.md) and the linked issue/PR authorization before requesting approval. A direct Product Owner instruction may authorize multiple named issues, dependent starts or technical merges; record its scope and exclusions once. Do not reimpose per-issue/per-merge stops already superseded by that delegation. Do not extend delegation to new issues, architecture decisions, spending or excluded gates by inference; the standing delegation above is explicit, not inferred.

## What does not need a gate

Reversible work inside an authorized issue: creating its branch, commits, a draft PR, tests, local experiments, documentation updates within scope, and replying to review findings.

## How an assistant asks

When a gate is reached, the assistant stops and says, in one place (the PR or issue, and the project thread): what decision is needed, the options, its recommendation, and exactly what the Product Owner needs to click or reply.

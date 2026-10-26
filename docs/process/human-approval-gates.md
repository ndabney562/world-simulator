# Human Approval Gates

These decisions belong to the Product Owner alone. An AI collaborator may propose them, but never acts on them without the Product Owner's explicit approval, written on GitHub (a PR approval, merge, or comment) or given directly to the assigned assistant and then recorded on the relevant issue or PR.

| Gate | Examples |
|---|---|
| **Merging into `main`** | Every PR. AI collaborators never merge unless the Product Owner has delegated merging for a named work stream and recorded that on the relevant issue or PR; a delegated merge still requires green CI and a clean review. |
| **Starting an engineering run** | Beginning implementation of an issue, or of the next issue after a PR is opened. |
| **Architecture changes** | Accepting, superseding, or deprecating an ADR; any change that contradicts an accepted ADR or the architecture guardrails. |
| **Scope changes** | Adding, removing, or re-scoping a stage, or moving work between stages; changing an issue's acceptance criteria after work starts. |
| **Test and CI weakening** | Deleting, skipping, disabling, or loosening a test or CI check; lowering a performance budget or tolerance; updating Tier 1 golden values. |
| **Cost** | Anything that may cost money: paid services, paid tiers, API usage above free tiers, paid assets or plugins (ADR 0007). |
| **External services and credentials** | Adding a third-party service, API key, secret, or account; changing Google or other provider quotas. |
| **Licences and data** | Adding a data source, asset, or dependency with a licence that is not permissive (MIT, Apache-2.0, BSD, Zlib, or similar), and every new data source (it must be registered in `docs/data/`). |
| **Apple and devices** | Signing, provisioning, TestFlight, App Store, anything using the Apple developer account; on-device runs on the Product Owner's iPhone. |
| **Repository settings** | Default branch, branch protection, rulesets, Actions permissions, collaborators, visibility. |
| **Irreversible actions** | Force-pushing or rewriting `main`, deleting branches with unmerged work, deleting issues or releases. |
| **Stage exit** | Declaring a stage (for example the Technical Prototype) complete. |

## Recorded delegation

Check [current-workstream.md](current-workstream.md) and the linked issue/PR authorization before requesting approval. A direct Product Owner instruction may authorize multiple named issues, dependent starts or technical merges; record its scope and exclusions once. Do not reimpose per-issue/per-merge stops already superseded by that delegation. Do not extend delegation to new issues, architecture decisions, spending or excluded gates by inference.

## What does not need a gate

Reversible work inside an authorized issue: creating its branch, commits, a draft PR, tests, local experiments, documentation updates within scope, and replying to review findings.

## How an assistant asks

When a gate is reached, the assistant stops and says, in one place (the PR or issue, and the project thread): what decision is needed, the options, its recommendation, and exactly what the Product Owner needs to click or reply.

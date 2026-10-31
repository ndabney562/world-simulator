# Desktop preview publication record (#112)

Status: **published through GitHub Pages on 2026-10-28**.

Play: [https://ndabney562.github.io/world-simulator/](https://ndabney562.github.io/world-simulator/). Publication commit
`bf1493399c5149183757646c2b4ce0bd1f70c83b` on `gh-pages`; Pages serves its root
with HTTPS enforced. On 2026-10-30, public HTTPS verification confirmed the pinned manifest and all
nine player file sizes/hashes, plus matching publication provenance. `.nojekyll`
is the branch hosting marker.

The record below describes the bounded action that was authorized and performed. This is a limited desktop
preview, not all-device support or native iPhone stage acceptance.

## Concrete artifact

Runtime commit `8de9a2cb393383a61d2e66ca3ce4d1441a2de424`, source tree
`c02ea2cb27d9b4496bf31b96f80b230e81c9db17`. The complete nine-file player is
15,080,019 bytes, plus its manifest and two static hosting/provenance files.
Manifest SHA-256: `9e24465401907ba6683687bbe86044f2ea5bdfbd72cf31178ea031066d4cb508`.
Local distribution ZIP SHA-256:
`97f42bfefee94bbeea6c4ae4ac3b26dd8324a5ac5cea96c32bc5d4942a5c3fc6`.

`tests/web/release/prepare_pages_candidate.py` validates that exact reviewed
manifest, every file size/hash, required notices and absence of the panic marker,
then copies only the player into a new local candidate directory. It adds an empty
`.nojekyll` and nonprivate `publication-info.json`. No probe, save, profile, build
cache, raw test log or source checkout is included. Candidate preparation never
uploads, pushes or changes repository settings.

## Publication action and settings

Noah approved GitHub Pages publication on 2026-10-28 and reconfirmed that
approval on 2026-10-30. Publication/settings approval covered uploading only
that candidate to the new `gh-pages` branch of this public repository. The remote branch now points to the
publication commit above. Existing Git authentication passed a push dry run; no new account,
secret or credential is proposed. Retain original-code MIT text and collected
third-party/data/Rust/Unity notices exactly as verified in the player.

Pages configuration: **Settings → Pages → Deploy from a branch → gh-pages → /(root)**.
This one-time publication/settings approval was separate from technical merge delegation.
No cloud Unity build or licence secret is needed; the already-verified local
export is the deployed artifact. No other repository setting, new account or
secret was introduced.

Publication approval and the deployment commit are recorded on
[issue #112](https://github.com/ndabney562/world-simulator/issues/112#issuecomment-6484964072).
On 2026-10-30, the Pages API reported `built`, HTTPS enforced and source
`gh-pages` / root; the remote branch still matched the publication commit.
Public HTTPS checks under the repository prefix verified the manifest SHA-256,
all nine player sizes/hashes and the runtime commit/tree in `publication-info.json`.
These HTTP/hash checks do not exercise rendering or browser persistence.
Prepublication rendering/save/reload/import evidence remains in the
[release checks](release-checks.md).

Noah reported a public-origin smoke test on 2026-10-30 in his desktop browser at
[the live preview](https://ndabney562.github.io/world-simulator/): saving worked, and the saved world
loaded after a page reload. Backup export/import on the live site was **not
tested**. Other devices were **not tested** in this smoke test. This owner-reported
result does not extend the prepublication export/import or device evidence.

## What stays separate

Installed Safari27.0.1 passed manual visual/save/reload and intentional-panic
recovery checks; its automation experiments are excluded, and no sustained Safari
FPS or native-picker guarantee is claimed. Another physical desktop/Windows/Linux, Intel or low-memory
graphics, phone browsers, cold-start offline caching and native iPhone criteria
remain explicit preview limits. [Release evidence](release-checks.md) records exact
engine results and their measurement limits. Browser saves belong to each origin
and device: download a local backup and import it on the public origin.
There is no multiplayer/cloud save service or paid runtime API introduced here.

The prepublication QA, independent actual-diff review and all seven required CI
checks passed before publication. No PR/code review was requested from the owner;
the engineer handled technical review and delegated merges. This publication does
not declare a native iPhone stage complete.

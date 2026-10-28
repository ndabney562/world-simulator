# Desktop preview publication proposal (#112)

Status: **prepared locally; not approved or published**. This is a limited desktop
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

## Proposed publication action after the remaining gate

With explicit owner publication/settings approval, upload only that candidate to
a dedicated `gh-pages` branch of this public repository. That branch currently
does not exist. Existing Git authentication passed a push dry run; no new account,
secret or credential is proposed. Retain original-code MIT text and collected
third-party/data/Rust/Unity notices exactly as verified in the player.

Select repository **Settings → Pages → Deploy from a branch → gh-pages → /(root)**.
This one-time repository setting is separate from technical merge delegation.
No cloud Unity build or licence secret is needed; the already-verified local
export is the deployed artifact. Do not push the branch or enable Pages until
approval is recorded.

Expected project URL: `https://ndabney562.github.io/world-simulator/`.
It is a proposed URL, not a claimed working link. After deployment, verify HTTPS,
HTTP statuses and every deployed file hash, gzip fallback under the repository
prefix, real rendered game, save/reload/import, credits and the deployment's
provenance file. Share the link only after this public smoke test succeeds.

## What stays separate

Installed Safari still needs a reliable visual check; its automation experiments
are not a player pass. Another physical desktop/Windows/Linux, Intel or low-memory
graphics, phone browsers, cold-start offline caching and native iPhone criteria
remain explicit preview limits. [Release evidence](release-checks.md) records exact
engine results and their measurement limits. Browser saves belong to each origin
and device: download a local backup and import it on the eventual public origin.
There is no multiplayer/cloud save service or paid runtime API introduced here.

The owner approves publication only after remaining QA, independent actual-diff
review and latest-head checks are complete. No PR/code review is requested from
the owner; the engineer handles technical review and delegated merges.

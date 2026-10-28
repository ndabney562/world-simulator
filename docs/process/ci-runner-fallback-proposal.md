# Temporary CI runner fallback (#117)

Status: **Approved for implementation.** [Owner authorization](https://github.com/ndabney562/world-simulator/issues/117#issuecomment-6478751625). Latest-head green CI, independent review and exact-source local Apple Silicon evidence remain required; this policy does not delegate merges.

## Evidence

- macOS15 and macOS26 Apple Silicon jobs were cancelled before acquisition with explicit hosted arm64 capacity notices. Retrying or changing the Mac OS version did not provide a runner.
- Standard free Intel Mac run [40744059494](https://github.com/ndabney562/world-simulator/actions/runs/40744059494) at `db6c0e4c7d4b261e7e0071a313900000ec42517c` passed both the full core/native C/replay/save-load job and the header/iOS archive job.
- ARM Linux passed the full core/native C/replay/save-load/dependency checks at `ada48b52529c6dceaee52bf21b67cd2c509866c9` in [40752189191](https://github.com/ndabney562/world-simulator/actions/runs/40752189191). It is not Apple Silicon/macOS verification.
- Browser candidate #114 separately has reviewed local Apple Silicon verification. Local results must correspond to the actual native source under review; results from a different native source revision cannot be substituted.

## Coverage policy

Temporarily require the same complete Rust test commands, native C consumer, replay/save-load and dependency checks across the following hosted matrix, keeping all pinned versions, goldens and failure conditions:

| Hosted runner | Native coverage |
|---|---|
| `ubuntu-latest` | Linux x86-64 |
| `ubuntu-24.04-arm` | Linux ARM64 |
| `macos-15-intel` | macOS x86-64; also existing header generation and cross-compiled `aarch64-apple-ios` archive |

Additionally require local Apple Silicon Rust/native-bridge checks for native code changes, with exact source revision and evidence recorded before technical merge. Retain native iPhone acceptance as a separate device gate. No self-hosted runner, new account, secret, paid machine or signing is introduced.

This changes where Apple Silicon-specific verification runs: local rather than GitHub-hosted. The hosted macOS ARM64 combination is missing, so this cannot be adopted silently under a claim of equivalent hosted coverage. The approved change is bounded by the test/CI and scope gates in [human-approval-gates](human-approval-gates.md). Existing check identities remain compatible: `Rust core (macos-15)` now selects the Intel image; `Native bridge and header` also runs there. Any repository-settings change needs its own approval.

## Adoption conditions and exclusions

The ARM Linux diagnostic passed and the coverage policy is approved. The exact implementation diff must still receive independent review. Run `bash scripts/verify-local-apple-silicon.sh` and attach full output identifying the tested commit and clean native source tree before technical merge of native changes. Any implementation still needs latest-head green checks and the required local native evidence. Failed or missing checks are not marked successful.

The owner may restore hosted Apple Silicon Mac jobs once capacity is demonstrably available; record that follow-up explicitly rather than promising an automatic transition. Game publication, merge delegation, paid services and Apple device/account actions remain separate gates. Code review remains the engineers' responsibility.

# Proposed temporary CI runner fallback (#117)

Status: **Proposed; not adopted.** The current diagnostic workflow retains all Apple Silicon Mac checks. No merge or CI waiver is authorized by this document.

## Evidence

- macOS15 and macOS26 Apple Silicon jobs were cancelled before acquisition with explicit hosted arm64 capacity notices. Retrying or changing the Mac OS version did not provide a runner.
- Standard free Intel Mac run [40744059494](https://github.com/ndabney562/world-simulator/actions/runs/40744059494) at `db6c0e4c7d4b261e7e0071a313900000ec42517c` passed both the full core/native C/replay/save-load job and the header/iOS archive job.
- ARM Linux comparison is being collected through an additive `ubuntu-24.04-arm` row. It is not Apple Silicon/macOS verification and must not be described as such.
- Browser candidate #114 separately has reviewed local Apple Silicon verification. Local results must correspond to the actual native source under review; results from a different native source revision cannot be substituted.

## Concrete proposal for Noah's decision after diagnostics

Temporarily require the same complete Rust test commands, native C consumer, replay/save-load and dependency checks across the following hosted matrix, keeping all pinned versions, goldens and failure conditions:

| Hosted runner | Native coverage |
|---|---|
| `ubuntu-latest` | Linux x86-64 |
| `ubuntu-24.04-arm` | Linux ARM64 |
| `macos-15-intel` | macOS x86-64; also existing header generation and cross-compiled `aarch64-apple-ios` archive |

Additionally require local Apple Silicon Rust/native-bridge checks for native code changes, with exact source revision and evidence recorded before technical merge. Retain native iPhone acceptance as a separate device gate. No self-hosted runner, new account, secret, paid machine or signing is introduced.

This changes where Apple Silicon-specific verification runs: local rather than GitHub-hosted. The hosted macOS ARM64 combination is missing, so this cannot be adopted silently under a claim of equivalent hosted coverage. Noah's approval is required under the test/CI and scope gates in [human-approval-gates](human-approval-gates.md). Existing required-check identities should remain compatible; any repository-settings change needs its own approval.

## Adoption conditions and exclusions

The added ARM Linux check must pass, the exact implementation diff must receive independent review, and Noah must explicitly approve the temporary coverage policy before replacing any retained Mac ARM job. Any implementation still needs latest-head green checks and the required local native evidence. Failed or missing checks are not marked successful.

The owner may restore hosted Apple Silicon Mac jobs once capacity is demonstrably available; record that follow-up explicitly rather than promising an automatic transition. Game publication, merge delegation, paid services and Apple device/account actions remain separate gates. Noah is deciding the coverage policy, not being asked to inspect code or review a PR.

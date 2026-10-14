---
name: Bug
about: Something behaves differently from its specification, test, or ADR
title: "bug: <short description>"
labels: ["type: bug"]
---

## Summary
<!-- One sentence: what is wrong. -->

## Expected behaviour
<!-- What should happen, with the spec, test, issue, or ADR that says so. -->

## Actual behaviour
<!-- What happens instead. Paste exact output, logs, or failing test names. -->

## Reproduction
<!-- Exact steps or command, including seed, commit SHA, platform, and save file if any. -->
1. 

## Environment
- Commit:
- Platform (Linux / macOS arm64 / iPhone model + iOS version / Unity version):
- Determinism tier involved (1 / 2 / 3 / 4 / unknown):

## Impact
<!-- What it breaks: saves, determinism, invariants, performance, UX. -->

## Required tests
- [ ] A regression test that fails before the fix and passes after

## Definition of Done
- [ ] Meets [docs/process/definition-of-done.md](https://github.com/ndabney562/world-simulator/blob/main/docs/process/definition-of-done.md)
- [ ] Root cause stated in the PR

# Branch, Commit, and PR Conventions

## Branches

- `main` is the default branch. It is always green, and it changes only through merged PRs.
- One branch per issue, created from the current `main`:

  ```
  <author>/<issue-number>-<short-slug>
  ```

  Examples: `claude/2-tp-001-core-skeleton`, `chatgpt/14-adr-ecs-choice`, `noah/20-bay-ridge-bounds`.
- When a tool assigns the branch name (for example a Claude Code web session assigns `claude/<name>-<random>`), use the assigned branch and put the issue ID in the PR title and description instead.
- Branches are deleted after merge.
- Spike branches use `spike/<issue-number>-<slug>` when their code is not meant to merge.

## Commits

Format, based on Conventional Commits:

```
<type>(<scope>): <summary in the imperative, ≤ 72 characters>

<body: what changed and why, wrapped at 72 characters>

Refs #<issue>
<trailers>
```

| Type | Use |
|---|---|
| `feat` | New behaviour |
| `fix` | Bug fix |
| `test` | Tests only |
| `docs` | Documentation only |
| `adr` | Proposing or accepting an ADR |
| `ci` | CI configuration |
| `build` | Build system, toolchain, dependencies |
| `refactor` | Code change without behaviour change |
| `perf` | Performance improvement |
| `chore` | Housekeeping |

- `scope` is the crate (`ws-time`), the area (`unity`, `ffi`, `process`), or omitted.
- One logical change per commit. Formatting-only changes are separate commits.
- AI-authored commits end with the tool's attribution trailers (for example `Co-Authored-By:`).
- No model identifiers, secrets, or credentials in commits.
- Once a review has started, fixes are new commits; history on a branch under review is not rewritten.

## Pull requests

- Title: `TP-001: Rust simulation-core skeleton` for Technical Prototype issues, otherwise `<type>(<scope>): <summary>`.
- Description: follows [`.github/pull_request_template.md`](../../.github/pull_request_template.md) and contains `Closes #<issue>`.
- One issue per PR. A PR that closes several issues needs the Product Owner's agreement first.
- Normal PRs are squash-merged; the squash commit message keeps the PR title and `Closes #<issue>`.

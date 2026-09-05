---
name: git-workflow
description: Git conventions for this user's repositories — a worktree per branch, short Conventional Commit messages, semver with annotated tags, and landing through a pull request. Use when branching, committing, opening a pull request, cutting a release, or writing a changelog entry.
---

# Git workflow

Standing preferences for every repository. Where a repository documents
something stricter of its own — a `CONTRIBUTING.md`, a release checklist —
that wins.

## Branch in a worktree, never on `main`

One worktree per line of work, made with `EnterWorktree`. Where it lands is
the tool's business (`git worktree list` says; with sinteractive's hooks
registered it is on the cluster's scratch on Alpine). Never symlink
`.claude/worktrees` elsewhere — Claude Code refuses to create a worktree
through a symlink — and make sure the repository ignores it. Leave with
`ExitWorktree`: `keep` while the branch is in flight, `remove` once merged.

Branch names are short kebab-case topics: `fix-stale-time-budget`,
`ci-validate-shell`.

## Commit messages are short

```
type(scope): subject
```

Types: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `ci`, `perf`,
`build`, `revert`. The scope is optional and names the area touched. The
subject is imperative, lowercase, no trailing period, under 72 characters,
and completes "this commit will …".

**Most commits are the subject line alone.** Add a body only when the diff
cannot say why — a non-obvious cause, a constraint that forced the shape —
and keep it to two or three lines. No narrative of what changed, what else
was tried, or how the problem was found. Mark a major bump with `!` before
the colon or a `BREAKING CHANGE:` footer.

Match a repository whose history plainly follows another convention, and
never rewrite history that is on the remote.

## Land through a pull request

Nothing goes onto `main` directly — not a typo fix, not a version bump.
Before pushing, run what CI runs (`.github/workflows/*.yml`: usually a
formatter, a linter, tests); when those are real compute, give them their
own allocation (`hpc-compute`).

```bash
git push -u origin HEAD
gh pr create --fill            # body: one to three sentences on why
gh pr checks --watch
gh pr merge --squash           # once green
```

Committing, pushing, opening the PR and watching CI is mechanical: the
`land` skill does it in a forked subagent on a cheaper model. Invoke it with
a one-line why once the work and its checks are done, rather than spending
the main conversation on it.

## Versions and releases

`MAJOR.MINOR.PATCH`: MAJOR when existing usage breaks, MINOR for additions,
PATCH for fixes. Before 1.0.0, breaking changes go in MINOR.

To release: grep the current version string across the repository and bump
every copy in one commit (a `VERSION=`, `Cargo.toml`, `pyproject.toml`, the
man page's `.TH` line, a docs config). In a Keep a Changelog file,
`[Unreleased]` becomes `[X.Y.Z] - YYYY-MM-DD` with a fresh empty
`[Unreleased]` above it, and the comparison links at the foot move. Commit
as `chore(release): vX.Y.Z` and land it through a pull request like
anything else. Then tag the merge commit on `main` — annotated, never
lightweight, so the tag records who cut it and when:

```bash
git checkout main && git pull
git tag -a v1.4.0 -m 'Release v1.4.0'
git push --follow-tags
gh release create v1.4.0 --generate-notes
```

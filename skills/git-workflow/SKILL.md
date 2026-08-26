---
name: git-workflow
description: Git conventions for this user's repositories — semantic versioning, Conventional Commit messages, one worktree per branch, and landing work through pull requests. Use whenever a task involves branching, committing, opening or reviewing a pull request, cutting a release, tagging a version, or writing a changelog entry.
---

# Git workflow

These are the user's standing preferences. They apply to whatever repository
is open, not to any one project. Where a repository documents something
stricter of its own — a `CONTRIBUTING.md`, a release checklist — that wins.

## Branch in a worktree, never on `main`

One worktree per line of work. The convention is
`<repo>/.claude/worktrees/<name>`, which is what the `EnterWorktree` tool
creates; prefer it over `git worktree add` so the session's working directory
follows the worktree instead of being left behind in the main checkout.

The base commit comes from the `worktree.baseRef` setting: `fresh` (the
default) branches from `origin/<default-branch>`, so the work starts from what
is actually on the remote rather than from whatever the local checkout has
drifted to; `head` branches from local `HEAD`, for work that genuinely builds
on uncommitted local history.

`.claude/worktrees/` is a byproduct of the workflow, not source. If the
repository does not already ignore it, add it to `.gitignore` — or to
`.git/info/exclude` when the ignore file is shared and the convention is not.

Leave with `ExitWorktree`: `keep` while the branch is still in flight,
`remove` once the pull request has merged. A worktree outliving its branch is
a checkout of something that no longer exists.

Branch names are short kebab-case topics describing the change, reading much
the way the subject line does: `fix-stale-time-budget`, `ci-validate-shell`,
`install-claude-from-anywhere`.

## Land through a pull request

```bash
git push -u origin HEAD
gh pr create --fill          # then edit the body to say why
gh pr checks --watch         # let CI go green before merging
gh pr merge
```

Nothing goes onto `main` directly — not a typo fix, not a version bump, not a
one-line revert. The pull request is where CI runs and where the reasoning is
recorded; a change that skips it has neither, and the gap only surfaces later,
when somebody asks why a line is the way it is.

Never force-push a branch someone else may have checked out, and never rewrite
history that is already on the remote.

## Conventional Commits

```
type(scope): subject

Body explaining why, wrapped at 72.

BREAKING CHANGE: what callers must now do differently.
```

Types: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `ci`, `perf`,
`build`, `revert`. The scope is optional and names the area touched
(`fix(nodes):`, `feat(claude):`); leave it off when the change is repo-wide.

The subject is imperative, lowercase, no trailing period, and completes the
sentence "this commit will …". The body explains *why* — the failure mode
being fixed, the alternative that was rejected and what was wrong with it —
because the diff already says what changed and nothing else records the
reasoning. A mechanical change needs no body; a judgment call always does.

Mark anything forcing a major version bump with either a `BREAKING CHANGE:`
footer or a `!` before the colon (`feat(api)!:`).

Where a repository's recent history plainly follows a different convention,
match the repository rather than switching styles mid-log — and never rewrite
existing commits to conform.

## Semantic versioning

`MAJOR.MINOR.PATCH`: MAJOR when existing usage breaks, MINOR for
backwards-compatible additions, PATCH for fixes that change no interface.
Before 1.0.0 the guarantee shifts down a place — MINOR is where breaking
changes go, and users should expect them there.

Tags are `v`-prefixed and **annotated**:

```bash
git tag -a v1.4.0 -m 'Release v1.4.0'
```

A lightweight tag is a bare pointer with no tagger, date, or message, so a
release cut that way leaves no record of when it was made or by whom.
Annotate every one.

## Releasing

**Find every place the version is written before changing any of them.** It is
routinely more than one: a `VERSION=` in a script, `pyproject.toml`,
`package.json`, a `DESCRIPTION`, the `.TH` line of a man page, a docs config.
Grep for the current version string across the repository and bump the whole
set in one commit — a stale copy is invisible until a user reports that
`--version` disagrees with the tag.

Then, for a repository keeping a changelog in Keep a Changelog form: rename
`## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`, open a fresh empty
`[Unreleased]` above it, and update the comparison links at the foot of the
file — `[Unreleased]` moves to `compare/vX.Y.Z...HEAD`, and a new `[X.Y.Z]`
link points at `compare/vPREV...vX.Y.Z`.

```bash
git commit -m 'chore(release): v1.4.0'
git tag -a v1.4.0 -m 'Release v1.4.0'
git push --follow-tags
gh release create v1.4.0 --generate-notes
```

A release is still a pull request. Tag the merge commit on `main`, not the
branch.

## Run the repository's own checks before pushing

Read `.github/workflows/*.yml` and run what CI runs, locally, first. The gates
are usually a linter, a formatter check, and a test suite, and they take
seconds by hand; discovering them from a red pull request costs a round trip
and leaves a failed run in the history for nothing.

When those checks are heavy enough to be real compute — a full test suite, a
build — they belong in their own Slurm allocation rather than in the session
shell. See the `bodhi-compute` skill.

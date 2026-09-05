---
name: land
description: Commit the finished work on the current branch with a short Conventional Commit, push, open the pull request, and watch CI — the mechanical end of a change, run in a forked subagent on a cheaper model. Invoke once the work is done and its checks pass, with a one-line why as the argument; add --merge to squash-merge when CI is green.
argument-hint: "[why] [--merge]"
context: fork
model: sonnet
effort: low
background: false
---

# Land the current branch

You are in a checkout on a topic branch. The change is finished and its
checks have passed; your job is to record and land it, nothing more. Do not
edit source files — if something needs changing, stop and say what.

The caller's reason for the change: **$ARGUMENTS**. If it is empty, derive
one from the diff. If it contains `--merge`, merge at the end.

1. **Look.** `git status --short`, `git diff HEAD`, and
   `git log --oneline origin/main..HEAD` (or the repository's default
   branch). Refuse if the branch *is* the default branch: say so and stop.
   Note whether recent `git log` follows a convention other than
   Conventional Commits; match the repository if it does.
2. **Commit.** Stage what belongs to the change — `git add -A` unless the
   status shows things that plainly do not (build output, data, anything
   that looks like a secret); leave those and mention them. Message:
   `type(scope): subject` — imperative, lowercase, no period, under 72
   characters. A body only when the why is not obvious from the subject,
   two or three lines at most. Keep whatever trailers this session's
   instructions require. Nothing to commit but unpushed commits: go on.
3. **Push.** `git push -u origin HEAD`.
4. **Pull request.** If `gh pr view --json url,state` shows one open for
   this branch, keep it. Otherwise `gh pr create` with the commit subject
   as title and a body of one to three sentences on why (plus the footer
   this session's instructions require, if any).
5. **CI.** `gh pr checks --watch --fail-fast`. If it will run more than a
   few minutes, run it in the background and wait for the notification.
   With `--merge` and every check green: `gh pr merge --squash --delete-branch`.
6. **Report** in under ten lines: commit subject, PR URL, CI result — or
   the failing check and the first lines of its error — and whether it
   merged.

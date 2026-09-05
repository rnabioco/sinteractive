---
name: model-routing
description: Pick the model and effort for a piece of work, and decide what to fork away from the main conversation. Covers why effort is the first lever and a model swap the last, what haiku and sonnet forks are for, and when the most capable model actually earns its cost. Use when a task is about to run long, when tempted to split a workflow across two models, or when the session is spending the main model on waiting.
---

# Choosing the model and the effort

A session's main conversation is the expensive place. Every tool result in
it replays everything said so far, so a wait, a queue check or a bulk grep
run from the main conversation is billed at the main model's price for work
that needs no judgment at all.

Two separate decisions, in this order: **what leaves the conversation**, and
**how hard the model works on what stays**. Changing model is a distant
third, and usually the wrong first move.

## What leaves the conversation

Fork it if it needs no judgment and no memory of the discussion:

- `job-watch JOBID…` — the wait for a Slurm job, on haiku.
- `land "why"` — commit, push, open the pull request, watch CI, on sonnet.
- Anything else mechanical — a queue survey, a quota check, a bulk grep, a
  sweep across the cluster — to `Agent` with `model: haiku`, or `sonnet`
  when it has to read code and decide something.

A fork has no conversation history, so the prompt must stand alone. Say what
to return, not how to get it. End a turn that is only waiting rather than
polling from it.

## Effort is the first lever

`effort` runs `low` → `medium` → `high` → `xhigh` → `max`, defaulting to
`high`. It trades thoroughness against token spend *within one model*, and
it moves quality further than most model swaps do.

| Phase | Effort | Why |
|---|---|---|
| Planning, design, writing the spec | `max` | Few tokens, no compiler to catch a bad call, and the error stays silent until the code exists. |
| Implementation, agentic runs | `xhigh` | Documented as the best setting for coding and agentic work, and Claude Code's own default. |
| Reviewing a diff | `max` | Same shape as planning — cheap in tokens, silent when wrong. |
| Subagents, mechanical sweeps | `low` | Fewer and more consolidated tool calls, less preamble. |

Raise to `max` only where measurement shows headroom at the level below.
Which workloads repay high effort is a property of the workload, not a
constant: coding and long-horizon agentic work respond strongly; chat,
classification and high-volume routes often do fine at `low`.

Judge cost per *completed task*, not per request. A cheaper request that
needs three more turns to finish the job is not cheaper.

## Why not to split a workflow across two models

**Caches are model-scoped.** Plan on one model and implement on another and
the implementing agent starts cold on the same repository the planner just
read — the cascade forfeits every cache read across the boundary, and pays
fresh input rates to rebuild what was already warm.

So before reaching for a second model, measure the simpler thing: the most
capable model at *lower* effort on the same tasks. Lower effort on a current
model often matches or beats a previous generation at high effort, and one
model means one cache namespace.

If a model swap does earn its place, escalate the **long-horizon** end, not
the bounded one. Writing a spec is a short task; executing it across hours
of tool calls is not, and that is where the most capable model's advantage
is claimed to lie. Escalate the runs that already failed, or that you know
are multi-hour and tool-heavy — an escalation policy, not a fixed
assignment, so you collect the evidence to tell whether it repaid you.

## Long-horizon runs

Give the **whole task spec up front**, then stop. Prompts written for older
models tend to be over-prescriptive, and on current models that measurably
*reduces* output quality — say what done looks like and what is out of
scope, not how to write each step.

Sizing, walltime and where the work runs are a different question — see
`hpc-compute`.

## Related skills

- `job-watch` — the wait for a Slurm job, on the cheapest model.
- `land` — the commit/PR/CI end of a change, on a cheaper model.
- `hpc-compute` — where the work itself runs, and how big.

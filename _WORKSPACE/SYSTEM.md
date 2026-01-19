# SYSTEM.md — Naia + Namako Dual-Repo Agent Operating Rules

## 0) Single Source of Truth (SSoT) Policy

Each document in `_WORKSPACE/` has a distinct responsibility. Do not duplicate content across docs.

| Document | Responsibility |
|----------|----------------|
| `SYSTEM.md` | Hard constraints (no git ops, repo hygiene, reporting discipline) |
| `CURRENT_STATUS.md` | Live dashboard: MODE, gates, commands, paths, current objective |
| `GOLD_PLAN.md` | Canonical product plan + normative rules (two FSMs, hash contracts, schemas) |
| `CLAUDE.md` | Short onramp that points to the above docs |

**For MODE/FSM state, always consult `CURRENT_STATUS.md` first; do not infer from other sources.**

---

## 1) Prime Directive
This workspace contains TWO separate git repositories:
- `naia/`   — the Naia project and its Namako integration (features, adapter, bindings, test harness)
- `namako/` — the Namako engine/tooling (fork of cucumber: engine/lib, proc-macro/codegen, CLI, schemas/hashing rules)

You may freely read and edit files in EITHER repo as needed to complete the task.

## 1) Git Operations Are Forbidden
You MUST NOT perform any git operations, including:
- commit, amend, rebase, merge
- checkout / switch branches
- pull, fetch, push
- reset, revert, cherry-pick
- tag, stash
- submodule operations
All git work is performed by Connor.

If a change implies a git operation, STOP and explain what Connor should do.

## 2) Always Use Repo-Prefixed Paths
When referencing files in plans or explanations, always prefix paths with:
- `naia/...`
- `namako/...`

Never write ambiguous paths like `src/lib.rs` without the repo prefix.

## 3) How to Decide Which Repo a Change Belongs In

### 3.1 Put it in `namako/` if it is any of:
- Namako engine behavior: feature parsing, step resolution, plan generation, verification logic
- NPA protocol schemas or validation logic
- Hashing / canonical encoding rules and implementations
- CLI commands (`namako lint/run/verify/update-cert/...`)
- Proc-macros / codegen (`#[given] #[when] #[then]` machinery, registry generation)
- Any “generic framework” change that should apply to multiple projects beyond Naia

### 3.2 Put it in `naia/` if it is any of:
- `.feature` files (specs) and certification artifacts in the Naia repo
- Naia-specific step bindings and their implementations
- The Naia adapter executable (NPA adapter): `manifest` + `run`
- Naia test harness integration (World type, Scenario wiring, local transport behavior)
- Anything that is project-specific and not reusable framework functionality

### 3.3 Cross-repo change rule
If a change alters an interface boundary (NPA schema/fields, hashing contract, plan format, etc.):
- Update `namako/` first (the authority)
- Then update `naia/` to conform (adapter output, tests, fixtures, etc.)
- Provide a short “cross-repo impact note” listing exactly what changed and what was updated.

## 4) Operating Procedure (Do This Every Time)

### Step A — Restate the objective + repo touch list
Before editing, write a short plan containing:
- What you’re trying to accomplish (1–3 sentences)
- Which repo(s) you expect to touch: `naia`, `namako`, or both
- A list of the key files you expect to edit (repo-prefixed paths)

### Step B — Ground truth scan
Search across BOTH repos for:
- existing implementations of the concept
- existing schemas/artifacts
- any docs that define the current behavior

If there’s a conflict between “what the spec says” and “what code does”, call it out explicitly.

### Step C — Make minimal, mechanical edits
Apply the smallest coherent change set that:
- removes contradictions
- makes behavior deterministic
- preserves v1 KISS constraints
- keeps v2+ upgrade paths open (version bumps, additive fields, etc.)

### Step D — Run checks (no git)
Prefer project-provided scripts (Makefile, justfile, task runner).
If none exist, do the simplest reasonable checks:
- In `namako/`: build + unit tests relevant to touched crates
- In `naia/`: build + tests relevant to adapter/bindings/features

If a command is uncertain, look for README/CONTRIBUTING first; otherwise propose the command and ask Connor to run it.

### Step E — Produce an “Execution Report”
After changes, output:
1) Files changed (repo-prefixed)
2) What changed and why (brief)
3) Any new invariants added
4) Any follow-ups Connor should do manually (including any git steps)

## 5) “Never Surprise the Spec”
If you change anything identity-critical (hashing, canonicalization rules, sorting rules, schema fields):
- Update the spec text (where appropriate)
- Ensure examples match the rules
- Ensure the verifying tool recomputes authoritative values (do not trust echoed fields)

## 6) Style Rules for Spec/Schema Work
- Use strict normative language: MUST / MUST NOT / SHOULD / MAY
- Avoid “implementation choice” in identity-critical rules (hashing, ordering, normalization, inclusion/exclusion)
- Prefer determinism over cleverness
- Prefer additive fields and version bumps over breaking rewrites
- Keep v1 constraints small and explicit; move complexity to v2+ as planned

## 7) Common Task Routing Examples
- “Fix canonical JSON rules / hashing determinism” → `namako/` (and update `naia/` outputs if impacted)
- “Update NPA manifest fields” → `namako/` schema + `naia/` adapter emission
- “Convert Markdown spec → .feature files” → `naia/`
- “Adapter refuses stale plans” → likely both (`namako/` defines + verifies, `naia/` enforces)
- “Step macro registry ordering” → `namako/`

## 8) When You Are Unsure
Do not guess silently. Instead:
- State the uncertainty
- Identify where in the code/spec the truth should live
- Propose the smallest clarifying change that makes it unambiguous

## Failure State Corpus v0.1

### Canonical schema

* **id**: stable unique identifier
* **layer**: where the contract is violated (Feature / Scenario / Binding / Runner / Tesaki / Repo / Validity / Ops)
* **name**: short canonical label
* **state**: one-sentence falsifiable condition
* **observables**: what we’d notice when in this state (symptoms, signals)
* **impact**: why it matters (what breaks / what gets worse)

---

## Bucket A — Feature intent & information flow (Gherkin context contracts)

**FM-A001 — Missing top-level intent**

* layer: Feature
* state: Top-level description/comments do not state the feature’s purpose, boundaries, and success criteria.
* observables: implementers disagree on what “done” means; scenarios feel arbitrary or “too local.”
* impact: drift + wasted scenario/test iteration.

**FM-A002 — Intent not delivered to agent**

* layer: Tesaki/Feature
* state: Authoritative feature intent exists but is not included in the mission bundle / agent-visible packets.
* observables: agent acts surprised by constraints humans assume are obvious; “why didn’t it do X?” moments.
* impact: repeated wrong-direction work.

**FM-A003 — Comment-rule inversion**

* layer: Feature
* state: Most normative requirements are embedded in prose comments, leaving Rules hollow.
* observables: rules read like headings; scenarios don’t “derive” from rules; review/explain output feels thin.
* impact: pipeline can’t enforce structure; scenario generation becomes vibes-based.

**FM-A004 — Rule overreach**

* layer: Feature
* state: A single Rule is effectively a full spec for the feature (too much scope in one Rule).
* observables: rule text long; scenarios sprawl; poor separation of concerns.
* impact: hard to certify incrementally; brittle edits.

**FM-A005 — Rule off-domain**

* layer: Feature
* state: A Rule describes behavior that belongs to a different feature/module contract.
* observables: rule references foreign concepts repeatedly; scenarios require unrelated setup.
* impact: coupling; unclear ownership; endless “where should this live?” churn.

**FM-A006 — Cross-file dependency blindness**

* layer: Feature/Tesaki
* state: A feature’s correct implementation depends on other contracts/features, but the mission does not surface them.
* observables: agent re-derives rules incorrectly; duplicate “definitions” appear across files.
* impact: divergent semantics across features; long-term incoherence.

---

## Bucket B — Rule structure & redundancy (within and across Rules)

**FM-B001 — Duplicate rule domains**

* layer: Feature
* state: Multiple Rules cover the same domain with overlapping requirements.
* observables: scenarios repeat with tiny variations; explanations mention same constraints in multiple places.
* impact: maintenance pain; inconsistent updates.

**FM-B002 — Rule needs splitting**

* layer: Feature
* state: One Rule spans multiple domains that should be separate Rules.
* observables: “and also” clauses; scenarios cluster into unrelated groups under one Rule.
* impact: poor traceability; hard gates.

**FM-B003 — Rule boundary ambiguity**

* layer: Feature
* state: It’s unclear which Rule owns a given scenario/requirement.
* observables: scenario could plausibly be under two Rules; reviewers disagree on classification.
* impact: broken “rules → scenario coverage” accounting.

---

## Bucket C — Scenario suite quality (coverage, redundancy, fidelity)

**FM-C001 — Insufficient scenarios for Rule**

* layer: Scenario
* state: A Rule has too few scenarios to credibly demonstrate it.
* observables: rule feels stronger than scenario evidence; edge cases absent.
* impact: false confidence; regressions slip through.

**FM-C002 — Low-quality scenarios**

* layer: Scenario
* state: Scenarios are underspecified, ambiguous, or not externally observable.
* observables: steps like “it works” / “state is correct”; assertions depend on internal details.
* impact: brittle tests or meaningless tests.

**FM-C003 — Scenario explosion / redundancy**

* layer: Scenario
* state: A Rule contains too many scenarios that are near-duplicates without adding coverage.
* observables: many scenarios differ only in constants; long runtime; high maintenance.
* impact: slow loop; humans stop reading; agents game by deleting.

**FM-C004 — Negative space missing**

* layer: Scenario
* state: Scenario set is mostly happy-path; adversarial and “should not happen” behaviors are absent where relevant.
* observables: catastrophic failures in real usage while tests remain green.
* impact: confidence mismatch.

**FM-C005 — Scenario-to-rule mismatch**

* layer: Scenario/Feature
* state: Scenarios do not actually test the Rule they are attached to.
* observables: scenario reads like it belongs elsewhere; explanations feel forced.
* impact: gates certify the wrong thing.

---

## Bucket D — Step vocabulary & bindings (semantic integrity)

**FM-D001 — Binding under-tests scenario**

* layer: Binding
* state: Bound steps fail to assert the scenario’s stated outcomes.
* observables: scenario passes even when outcome is broken; missing assertions.
* impact: silent regressions.

**FM-D002 — Binding over-tests / scope bleed**

* layer: Binding
* state: Bound steps test lots of extra behavior outside the scenario’s scope.
* observables: unrelated changes break tests; hard-to-explain failures.
* impact: brittle suite; discourages refactor.

**FM-D003 — Step collision (same phrase, different meaning)**

* layer: Binding
* state: Identical (or near-identical) step text is used across domains with incompatible semantics.
* observables: “fix” for one feature breaks another; confusion about intended meaning.
* impact: semantic corruption of step library.

**FM-D004 — Step explosion (tiny variants)**

* layer: Binding
* state: Many steps exist that are functionally identical but syntactically different.
* observables: step inventory grows fast; reuse stays low.
* impact: agent efficiency loss; maintenance drag.

**FM-D005 — Binding drift**

* layer: Binding
* state: Step implementation changes semantics over time while keeping the same step text.
* observables: old scenarios “still pass” but no longer mean what they meant; reviewers feel gaslit.
* impact: long-term trust collapse.

---

## Bucket G — Validity & “green lies” (tests pass, reality fails)

**FM-G001 — Hack-to-green**

* layer: SUT/Process
* state: Implementation reaches green via brittle hacks or poor architecture.
* observables: weird special cases; tight coupling; fear to touch code.
* impact: long-term velocity collapse.

**FM-G002 — Test gaming**

* layer: Validity
* state: Agent weakens assertions / reduces coverage to pass gates without implementing behavior.
* observables: fewer checks; “assert true” style outcomes; suspiciously easy pass.
* impact: certification becomes meaningless.

**FM-G003 — Spec incompleteness masquerading as correctness**

* layer: Validity/Feature
* state: Tests pass because spec never demanded the critical behavior.
* observables: real-world failures not representable as scenario failures.
* impact: false confidence; customer pain.

**FM-G004 — Adapter model mismatch**

* layer: Validity
* state: Adapter/harness observes a model that diverges from real system behavior.
* observables: certified behavior fails in integration; “but tests said…” confusion.
* impact: systemic trust failure.

# Path Forward: Rigorous Spec-to-Scenario Coverage

**Date:** 2026-02-03  
**Context:** Investigating how to achieve "just right" scenario coverage in autonomous SDD

---

## The Problem We're Solving

When running `tesaki --loop`, the `AddOrClarifyScenario` mission is:
- Adding scenarios ✅ (good)
- But ALSO adding new Rules ❌ (scope creep)
- Resulting in spec_issues going UP instead of DOWN
- The flywheel spins but makes things WORSE, not better

**Core Question:** What is the "just right" quantity of scenarios, and how do we systematically ensure we hit it?

---

## Research Findings

### The BDD Hierarchy (Gáspár Nagy, "Divide & Conquer à la BDD")

```
Feature (User Story / Goal)
    └── Rule (Acceptance Criterion / Business Rule)
            └── Scenario (Executable Example, 2-3 per rule)
```

**Key Insight:** Rules come from HUMANS (the spec). Scenarios are TESTS that prove the rules work.

### How Many Scenarios Per Rule?

From research on BDD best practices:

| Rule Complexity | Recommended Scenarios |
|-----------------|----------------------|
| Simple (one outcome) | 1-2 |
| Moderate (valid/invalid paths) | 2-3 |
| Complex (state, roles, calculations) | 3-6 |

**The "2-3 per rule" heuristic is widely cited as the sweet spot.**

Each rule should have scenarios covering:
1. **Positive path** (happy path, expected behavior)
2. **Negative path** (error handling, rejection)
3. **Edge cases** (boundaries, unusual inputs)

### LLM-Based BDD Generation Research

From "Comprehensive Evaluation of LLMs for BDD Acceptance Test Formulation" (2024):

1. **Few-shot prompting works best** - show examples of good scenarios
2. **Schema-aware prompts** - inject context about the domain/system
3. **Human-in-the-loop still necessary** - LLMs hallucinate and over-generate
4. **One-shot generation is valid** - LLM sees whole feature, generates complete coverage

---

## Current State Analysis

### What Namako Tracks

```rust
// From tesaki/src/issue_classifier.rs
pub fn classify_spec_issues(review: &ReviewPacket) -> Vec<SpecIssue> {
    // Creates a SpecIssue for each Rule with ZERO executable scenarios
    for rule in &feature.rules {
        if rule.executable_scenarios.is_empty() {
            issues.push(SpecIssue { kind: MissingCoverage, ... });
        }
    }
}
```

**Current metric:** `spec_issues.len()` = count of Rules with 0 scenarios

**Problem:** This is binary (0 scenarios = bad, 1+ = good). It doesn't track:
- Rules with only 1 scenario (might need more)
- Rules with 10 scenarios (probably over-tested)
- Whether the agent added NEW rules (scope creep)

### What's Going Wrong

1. Agent receives mission: "Add scenarios to `06_entity_scopes.feature`"
2. Agent reads feature, sees Rules that need coverage
3. Agent adds scenarios... but ALSO adds new Rules it thinks are missing
4. Result: Rule count goes UP, scenario count goes UP, but spec_issues also goes UP

**The agent is doing discovery work when it should be doing formulation work.**

---

## Proposed Solution: Rule-Count Invariant

### The Core Principle

> **During AddOrClarifyScenario, the Rule count MUST NOT increase.**
> 
> Rules are HUMAN-defined acceptance criteria. The agent's job is to add 
> SCENARIOS (tests) for those rules, not to invent new rules.

### Systematic Approach

#### Phase 1: Track Rule Count (Not Just Spec Issues)

Add to `RepoState`:
```rust
pub struct FeatureCoverage {
    pub feature_path: String,
    pub rule_count: usize,           // Human-defined rules
    pub scenario_count: usize,        // Total scenarios
    pub rules_with_zero_scenarios: usize,
    pub rules_with_one_scenario: usize,
    pub rules_with_adequate_coverage: usize, // 2+ scenarios
}
```

#### Phase 2: Stricter Progress Detection

For `AddOrClarifyScenario`:
```rust
let progress = scenarios_added > 0 && rules_with_zero_decreased;
let invariant_violated = rule_count_after > rule_count_before;

if invariant_violated {
    // Agent added new rules - this is FAILURE, not progress
    mark_mission_failed("Agent expanded scope by adding rules");
}
```

#### Phase 3: Better Mission Briefs

Current brief:
> "Add or clarify scenarios to improve coverage."

Proposed brief:
> **Objective:** Add 2-3 executable scenarios for each rule that currently has 0 scenarios.
>
> **Constraints:**
> - Do NOT add new Rules - only add Scenarios under existing Rules
> - Do NOT modify the Feature description or Rule text
> - Each scenario should cover a distinct case (positive, negative, edge)
>
> **Target Rules (need scenarios):**
> 1. Rule: "Owning client always sees own entity" (0 scenarios)
> 2. Rule: "Non-owning client respects scope" (0 scenarios)
>
> **Validation:** Rule count unchanged, scenarios added to listed rules.

#### Phase 4: One-Shot vs Incremental

**Research suggests one-shot is valid** - the LLM can see the whole feature and generate complete coverage.

Options:
1. **One-shot per feature:** Generate all scenarios for all rules in one mission
2. **One-shot per rule:** Generate 2-3 scenarios for one specific rule

Recommendation: **One-shot per feature** with explicit rule list and scenario targets.

---

## The Semantic Coverage Problem

### The Core Challenge

A Feature file contains:
```gherkin
Feature: Entity Scopes
  
  Entities can be scoped to specific clients. When an entity is scoped,
  only clients within that scope can see it. The owning client always
  sees its own entities regardless of scope settings.
  
  Rule: Owning client always sees own entity
    Scenario: ...
```

**Question:** How do we know the Rule + Scenarios adequately cover the intent in the Feature description?

This is a **semantic coverage** problem:
- The Feature description is natural language (human intent)
- Rules are structured acceptance criteria
- Scenarios are executable tests

**We need to verify:** Description → Rules → Scenarios (complete traceability)

### Research: LLM-Based Traceability Assessment

From NASA/MBSE research on AI-enhanced requirements traceability:

> "LLMs can bridge the semantic gap between high-level, context-dependent 
> requirement statements and their formal representation as specifications 
> or executable tests."

The Req2LTL framework achieves **88% semantic accuracy** mapping natural language to formal specs.

### Proposed Solution: Semantic Coverage Gate

Add a new gate phase: **Spec Coverage Assessment**

```
Before: namako gate = lint + run + verify
After:  namako gate = lint + run + verify + coverage-assess (optional)
```

The coverage assessment would:
1. Extract behavioral statements from Feature description
2. Map each statement to Rules
3. Map each Rule to Scenarios
4. Flag any statements without coverage

#### Option A: Heuristic Assessment (Simple)

```
For each Feature:
  - Parse description for action verbs + conditions
  - Count distinct behavioral statements
  - Compare to rule count
  - Flag if statements >> rules (missing rules)
  - Flag if rules >> statements (scope creep)
```

#### Option B: LLM Assessment (Semantic)

Use an LLM in REVIEW mode (not generation):

```
Prompt: Given this Feature description and its Rules/Scenarios, 
        assess coverage completeness.

Feature Description: [natural language]

Rules & Scenarios:
- Rule 1: [text] → Scenarios: [list]
- Rule 2: [text] → Scenarios: [list]

Questions:
1. Are there behaviors in the description not covered by any Rule?
2. Are there Rules that don't trace to the description (scope creep)?
3. Does each Rule have sufficient Scenarios (positive, negative, edge)?

Output: { coverage_score: 0-100, gaps: [...], excess: [...] }
```

#### Option C: Traceability Matrix (Rigorous)

Explicit linking:
```gherkin
Feature: Entity Scopes
  # @statement-1: Entities can be scoped to specific clients
  # @statement-2: Only clients within scope can see scoped entities  
  # @statement-3: Owning client always sees own entities
  
  Rule: Owning client always sees own entity
    # @traces: statement-3
    Scenario: Owner sees entity regardless of scope
      # @covers: positive-path
```

Then validate: all statements traced, all rules have 2+ coverage types.

### Recommendation

**Start with Option B (LLM Assessment)** as a separate mission type:

```rust
MissionType::AssessSpecCoverage {
    feature_path: String,
}
```

This mission:
1. Reads the feature file
2. Uses LLM to assess coverage
3. Outputs a coverage report (not code changes)
4. If gaps found → subsequent AddOrClarifyScenario missions target them

**Key insight:** This separates ASSESSMENT from GENERATION. The LLM reviews first, then acts.

---

## Implementation Plan

### Step 1: Enhance ReviewPacket Parsing

Ensure we can count:
- Rules per feature
- Scenarios per rule
- Total scenario coverage

### Step 2: Add Rule-Count Tracking to RepoState

```rust
pub struct RepoState {
    // Existing...
    pub spec_issues: Vec<SpecIssue>,
    
    // New: detailed coverage tracking
    pub feature_coverage: Vec<FeatureCoverage>,
    pub total_rules: usize,
    pub total_scenarios: usize,
}
```

### Step 3: Implement Rule-Count Invariant Check

In `repl.rs`, for `AddOrClarifyScenario`:
```rust
let rules_before = count_rules_in_feature(&before_state, &feature_path);
let rules_after = count_rules_in_feature(&after_state, &feature_path);

if rules_after > rules_before {
    println!("❌ INVARIANT VIOLATED: Agent added {} new rule(s)", 
        rules_after - rules_before);
    // Treat as mission failure
}
```

### Step 4: Improve Mission Brief Generation

Update `MissionType::AddOrClarifyScenario::generate_brief()`:
- List specific rules that need scenarios
- State explicit constraint: "Do NOT add new Rules"
- Set target: "2-3 scenarios per rule"

### Step 5: Consider "Spec Complete" Gate

Add a new gate phase or check:
```
Spec Complete = All rules have >= 2 scenarios
```

This gives a clear "done" signal for the spec refinement phase.

---

## Open Questions

### Q1: Should the human pre-define all Rules?

**Option A:** Human writes Feature + Rules, agent only adds Scenarios
- Pro: Clear separation of concerns
- Pro: Agent can't expand scope
- Con: Human must think of all acceptance criteria upfront

**Option B:** Agent can propose Rules, but human must approve
- Pro: Agent can help discover edge cases
- Con: Requires human review step (not fully autonomous)

**Recommendation:** Start with Option A. If the human spec is incomplete, the human should add Rules manually.

### Q2: What if a Rule genuinely needs 0 scenarios?

Some rules might be:
- Organizational (grouping other rules)
- Deferred (@deferred tag)
- Not testable at this level

**Solution:** Allow `@no-scenarios` or `@deferred` tags on Rules to exclude them from coverage checks.

### Q3: How to handle Scenario Outlines?

A Scenario Outline with 5 examples = 5 test cases from 1 scenario definition.

**Solution:** Count examples, not just scenario blocks. One Outline with 3 examples = adequate coverage for a simple rule.

---

## Success Metrics

After implementing these changes, a successful `tesaki --loop` should show:

```
MISSION 1: AddOrClarifyScenario
Target: features/06_entity_scopes.feature
Before: Rules=5, Scenarios=2, Coverage=40%
After:  Rules=5, Scenarios=12, Coverage=100%  ← Rule count UNCHANGED
Delta:  Scenarios +10, Rules +0
✅ Progress: All rules now have coverage

MISSION 2: CreateMissingBindings
...
```

**Key indicators:**
- Rule count stable or decreasing (consolidation OK)
- Scenario count increasing toward 2-3 per rule
- Spec issues trending to zero
- No "invariant violated" messages

---

## Appendix: Naia Feature File Analysis

### Current Structure (06_entity_scopes.feature)

The naia feature files have a rich structure:

```
Feature: Entity Scopes
  
  # NORMATIVE CONTRACT MIRROR (detailed spec in comments)
  # - Core scope predicate definitions
  # - Glossary of terms
  # - Behavioral requirements in prose
  
  Rule: Rooms gating                    # 2 scenarios
  Rule: Include/Exclude filter          # 3 scenarios  
  Rule: Owner scope invariant           # 2 scenarios
  Rule: Roomless entities               # 2 scenarios
  Rule: Scope state effects             # 1 scenario ← needs more
  Rule: Disconnect handling             # 1 scenario ← needs more
```

**Observation:** This feature has good structure (6 rules, 11 scenarios, ~2 per rule). The NORMATIVE CONTRACT comments provide excellent traceability material.

### Coverage Assessment Approach for Naia

Given the detailed comments in naia's feature files, we could:

1. **Extract behavioral statements** from the NORMATIVE sections
2. **Map each statement to a Rule** 
3. **Verify each Rule has adequate Scenarios**

Example mapping:
```
CONTRACT STATEMENT                              → RULE                        → SCENARIOS
"SharesRoom(U,E) MUST be necessary precondition"  → "Rooms gating"              → 2 ✓
"Owning client always in-scope for client-owned"  → "Owner scope invariant"     → 2 ✓
"Entity in zero rooms → OutOfScope for all"       → "Roomless entities"         → 2 ✓
```

This could be automated with an LLM in assessment mode.

---

## Immediate Next Steps

1. [ ] **Verify namako exposes rule count** in review packet
2. [ ] **Add rule-count tracking** to RepoState
3. [ ] **Implement invariant check** in repl.rs for AddOrClarifyScenario
4. [ ] **Improve mission brief** with explicit constraints and targets
5. [ ] **Test with one feature** to validate the approach
6. [ ] **Consider AssessSpecCoverage mission type** for semantic review
7. [ ] **Update plan.md** with progress

---

## References

- Gáspár Nagy: [Divide & Conquer à la BDD](https://gasparnagy.com/2019/05/divide-conquer-a-la-bdd-story-rule-scenario/)
- [Comprehensive Evaluation of LLMs for BDD Test Formulation](https://arxiv.org/pdf/2403.14965) (2024)
- [Agentic AI for BDD Testing using LLMs](https://www.researchgate.net/publication/389707150) (2025)
- [AI-Enhanced Requirements Traceability (NASA)](https://ntrs.nasa.gov/api/citations/20250008721/downloads/AI-Enhanced%20Requirements%20Traceability.pdf)
- [Bridging Natural Language and Formal Specification (Req2LTL)](https://arxiv.org/html/2512.17334v1)
- Cucumber Docs: [BDD Best Practices](https://cucumber.io/docs/bdd/)
- Matt Wynne: [Example Mapping](https://cucumber.io/blog/example-mapping-introduction/)

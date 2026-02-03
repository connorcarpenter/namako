# Path Forward: Rigorous Autonomous SDD for Naia

**Date:** 2026-02-03  
**Version:** 2.0  
**Goal:** Systematic, fully autonomous progress on Naia specs without human intervention

---

## Executive Summary

After critical analysis, the core problems are:

| Problem | Root Cause | Solution |
|---------|-----------|----------|
| spec_issues increase | Scenarios not "executable" until bindings exist | **Chain missions**: Scenarios → Bindings → Re-evaluate |
| Scope creep | Agent adds Rules, not just Scenarios | **Rule-count invariant** (deterministic gate) |
| Unknown "done" state | No criteria for adequate coverage | **2-scenario minimum** + **3-judge consensus** (for edge cases) |

**Philosophy:** Use deterministic checks for deterministic problems. Use LLM judgment only where semantic understanding is required.

---

## Part 1: The Executability Problem (Critical)

### The Issue

When `AddOrClarifyScenario` adds a new scenario, Namako doesn't count it as "executable" until ALL its steps have bindings. This creates a measurement problem:

```
Cycle N: AddOrClarifyScenario
  Result: 2 new scenarios with 6 new steps
  Namako says: executable_scenarios = 0 (steps have no bindings!)
  spec_issues: UNCHANGED or INCREASED
  
Cycle N+1: CreateMissingBindings
  Result: 6 bindings created
  Namako says: executable_scenarios = 2
  spec_issues: DECREASED
```

**Current progress detection fails** because it measures after scenario creation, before bindings exist.

### Solution: Mission Chaining (Deterministic)

**Insight:** AddOrClarifyScenario is INCOMPLETE until CreateMissingBindings runs.

```rust
/// AddOrClarifyScenario should NOT evaluate spec_issues directly.
/// Instead, it's successful if:
/// 1. New scenarios were added (parse the feature file)
/// 2. New binding issues were created (expected consequence)
/// 
/// Then IMMEDIATELY chain to CreateMissingBindings.

fn evaluate_add_scenario_progress(before: &State, after: &State) -> Progress {
    let scenarios_added = count_scenarios(&after) - count_scenarios(&before);
    let new_bindings_needed = after.binding_issues.len() - before.binding_issues.len();
    
    if scenarios_added > 0 {
        // Success: We added scenarios. New binding issues are EXPECTED.
        Progress::SuccessWithFollowUp {
            message: format!("Added {} scenarios, {} bindings now needed", 
                scenarios_added, new_bindings_needed),
            chain_to: MissionType::CreateMissingBindings { ... },
        }
    } else {
        Progress::NoChange
    }
}
```

**Key principle:** A compound operation (add scenarios + bind them) should be treated as a single unit of work.

---

## Part 2: The Scope Creep Problem

### The Issue

Agent is asked to add Scenarios but also adds Rules. This is scope expansion.

### Solution: Rule-Count Invariant (Deterministic)

This is a **hard gate**, not an LLM judgment. Rule count is a deterministic property.

```rust
/// Before AddOrClarifyScenario mission:
let rules_before = count_rules_in_feature(feature_path);

/// After mission completes:
let rules_after = count_rules_in_feature(feature_path);

if rules_after > rules_before {
    return MissionOutcome::Rejected {
        reason: format!(
            "INVARIANT VIOLATED: Rule count increased from {} to {}. \
             Agent must NOT add new Rules.",
            rules_before, rules_after
        ),
        action: RejectAction::RevertAndRetry,
    };
}
```

**No LLM needed.** This is a simple before/after comparison.

### Implementation

1. **Parse feature file before mission** → extract rule count
2. **Parse feature file after mission** → compare rule count  
3. **Reject if increased** → revert changes, retry with stricter prompt

### Stricter Mission Brief

```markdown
## Constraints (MUST NOT VIOLATE)

⛔ Do NOT add new `Rule:` blocks — only add Scenarios under EXISTING Rules
⛔ Do NOT modify the Feature description
⛔ Do NOT modify existing Rule text

Validation will REJECT your changes if rule count increases.
```

---

## Part 3: Coverage Completeness (Where LLM Judgment Helps)

### The Remaining Question

Once we fix executability and scope creep, we still need to know: **when is a feature "adequately covered"?**

This IS a semantic question that benefits from LLM judgment.

### The Simple Heuristic (Try First)

Before reaching for LLM judgment, apply a simple heuristic:

```rust
fn is_rule_adequately_covered(rule: &Rule) -> bool {
    rule.executable_scenarios.len() >= 2
}

fn is_feature_complete(feature: &Feature) -> bool {
    feature.rules.iter().all(|r| is_rule_adequately_covered(r))
}
```

**Rationale:** BDD research consistently recommends 2-3 scenarios per rule (positive path, negative path, edge case). 2 is the minimum for any meaningful rule.

### When LLM Judgment Is Needed

The heuristic fails for:
1. **Complex rules** that genuinely need 4+ scenarios
2. **Simple rules** where 1 scenario is sufficient
3. **Assessing scenario quality** (are these the RIGHT scenarios?)

For these cases, use **LLM-as-judge**.

---

## Part 4: LLM-as-Judge Consensus (For Coverage Quality)

### Design Principles (From Research)

| Principle | Source | Application |
|-----------|--------|-------------|
| Multiple judges reduce bias | arXiv 2404.18796 | Use 3 judges, not 1 |
| Pointwise > Pairwise | OpenReview | Score against rubric, don't compare |
| Locked rubrics prevent drift | Rulers (arXiv) | Fixed, versioned criteria |
| Self-consistency works | Google Research | Same model, 3 samples, majority vote |

### The Simplest Effective Approach: Self-Consistency

**Use the same model 3 times with temperature > 0, take majority vote.**

This is simpler than managing 3 different model APIs, and research shows it's nearly as effective.

```rust
fn assess_coverage(feature: &Feature) -> CoverageVerdict {
    let prompt = format_assessment_prompt(feature);
    
    // Three independent samples from same model
    let verdicts = [
        invoke_llm(&prompt, temperature: 0.3),
        invoke_llm(&prompt, temperature: 0.5),
        invoke_llm(&prompt, temperature: 0.7),
    ];
    
    // Majority vote
    let adequate_count = verdicts.iter().filter(|v| v.is_adequate()).count();
    
    if adequate_count >= 2 {
        CoverageVerdict::Adequate
    } else {
        CoverageVerdict::Inadequate {
            gaps: merge_gaps(&verdicts),
        }
    }
}
```

### The Locked Rubric

Each judge answers these questions (Yes = 1, Partial = 0.5, No = 0):

```markdown
## Coverage Assessment Rubric

Score each criterion (0, 0.5, or 1):

1. **Rule Coverage**: Does every Rule have at least 2 executable scenarios?
2. **Path Diversity**: Does each Rule have both positive and negative test paths?
3. **Edge Cases**: Are boundary conditions and edge cases tested?
4. **Traceability**: Can each Rule be traced to the Feature description?
5. **No Scope Creep**: Are all Rules justified by the Feature description?

ADEQUATE = Total score >= 4.0 / 5.0 (80%)
```

### When to Invoke

**Only invoke LLM assessment when the heuristic is ambiguous:**

```rust
fn should_invoke_llm_assessment(feature: &Feature) -> bool {
    let rules_with_1_scenario = feature.rules.iter()
        .filter(|r| r.executable_scenarios.len() == 1)
        .count();
    
    let rules_with_many_scenarios = feature.rules.iter()
        .filter(|r| r.executable_scenarios.len() > 4)
        .count();
    
    // Only assess if we have edge cases
    rules_with_1_scenario > 0 || rules_with_many_scenarios > 0
}
```

**Cost optimization:** Most features will pass the simple heuristic. LLM assessment is the exception, not the rule.

---

## Part 5: The Complete Autonomous Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                    TESAKI AUTONOMOUS LOOP                           │
│                                                                     │
│  1. COMPUTE STATE                                                   │
│     ├─ Run namako review → parse packets                            │
│     ├─ Count: spec_issues, binding_issues, sut_issues               │
│     └─ Count: rules per feature, scenarios per rule                 │
│                                                                     │
│  2. SELECT MISSION (Priority Order)                                 │
│     ├─ SUT issues → FixRegression                                   │
│     ├─ Binding issues → CreateMissingBindings                       │
│     ├─ Spec issues (rules with 0 scenarios) → AddOrClarifyScenario  │
│     └─ All green → AssessSpecCoverage (if heuristic ambiguous)      │
│                                                                     │
│  3. EXECUTE MISSION                                                 │
│     ├─ Generate brief with explicit constraints                     │
│     ├─ Invoke runner (Copilot)                                      │
│     └─ Capture changes                                              │
│                                                                     │
│  4. VALIDATE (Deterministic Gates)                                  │
│     ├─ Rule-count invariant: rules_after <= rules_before            │
│     ├─ Compile check: cargo build succeeds                          │
│     └─ Lint check: namako lint passes                               │
│                                                                     │
│  5. EVALUATE PROGRESS                                               │
│     ├─ For AddOrClarifyScenario: scenarios_added > 0 → chain to     │
│     │   CreateMissingBindings                                       │
│     ├─ For CreateMissingBindings: binding_issues decreased          │
│     └─ For FixRegression: sut_issues decreased                      │
│                                                                     │
│  6. CONTINUE OR STOP                                                │
│     ├─ Progress made → continue                                     │
│     ├─ 3 consecutive stalls → stop                                  │
│     └─ All issues at 0 + coverage heuristic passes → DONE           │
│                                                                     │
│  7. (Optional) COVERAGE QUALITY CHECK                               │
│     └─ If heuristic ambiguous: 3-judge self-consistency assessment  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Part 6: Implementation Checklist

### Phase 1: Fix Executability Problem (Highest Priority)
- [ ] **Modify `has_progress()` for AddOrClarifyScenario** 
  - Success = scenarios added (count the feature file)
  - Don't check spec_issues (they won't decrease until bindings exist)
- [ ] **Implement mission chaining**
  - AddOrClarifyScenario → auto-queue CreateMissingBindings
  - Evaluate spec_issues only after chain completes
- [ ] **Add scenario counting to RepoState**
  - `scenarios_per_rule: HashMap<String, usize>`

### Phase 2: Implement Rule-Count Invariant
- [ ] **Add rule counting to pre-mission snapshot**
  - Parse feature file → count `Rule:` lines
- [ ] **Add rule-count validation in post-mission**
  - Compare before/after
  - Reject if increased
- [ ] **Update AddOrClarifyScenario brief**
  - Explicit constraint: "Do NOT add new Rules"
  - Explicit validation warning

### Phase 3: Coverage Heuristic
- [ ] **Implement simple coverage check**
  - `rules_with_adequate_coverage()` = rules with 2+ scenarios
  - `is_feature_complete()` = all rules adequate
- [ ] **Add to DONE criteria**
  - Current: spec_issues == 0 && binding_issues == 0 && sut_issues == 0
  - New: && all features complete by heuristic

### Phase 4: LLM Coverage Assessment (Optional Enhancement)
- [ ] **Implement self-consistency assessment**
  - 3 samples, majority vote
  - Only invoke when heuristic is ambiguous
- [ ] **Create locked rubric prompt**
  - 5 criteria, 0/0.5/1 scoring
  - Threshold: 80% = adequate
- [ ] **Track assessment costs**
  - Log token usage per assessment
  - Should be ~$0.10-$0.30 per feature

---

## Part 7: Why This Works (No Human in Loop)

### Deterministic Where Possible

| Check | Type | Human Needed? |
|-------|------|---------------|
| Rule count unchanged | Deterministic (string parsing) | ❌ No |
| Scenarios added | Deterministic (file diff) | ❌ No |
| Bindings created | Deterministic (registry check) | ❌ No |
| Code compiles | Deterministic (cargo build) | ❌ No |
| Tests pass | Deterministic (cargo test) | ❌ No |
| 2+ scenarios per rule | Deterministic (counting) | ❌ No |

### LLM Judgment Only for Semantics

| Check | Type | When Used |
|-------|------|-----------|
| Scenario quality | Semantic (LLM) | When heuristic ambiguous |
| Coverage completeness | Semantic (LLM) | Edge cases only |

### Error Recovery (Autonomous)

| Failure | Autonomous Response |
|---------|---------------------|
| Rule-count increased | Revert, retry with stricter prompt |
| No scenarios added | Log, try different approach |
| Compile fails | Parse errors, include in next mission context |
| 3 consecutive stalls | Stop loop, output diagnostic |

---

## Part 8: Success Metrics

After implementation, `tesaki --loop 20` should show:

```
MISSION 1: AddOrClarifyScenario (features/06_entity_scopes.feature)
  Before: Rules=6, Scenarios=8, Coverage=67%
  Action: Added 4 scenarios to 2 rules
  After:  Rules=6, Scenarios=12, Coverage=100%
  ✅ Chaining to CreateMissingBindings...

MISSION 2: CreateMissingBindings (12 steps)
  Before: Binding issues=12
  Action: Created 12 bindings
  After:  Binding issues=0
  ✅ Progress: All bindings created

MISSION 3: FixRegression (test failure)
  Before: SUT issues=1
  Action: Fixed assertion in entity_scopes.rs
  After:  SUT issues=0
  ✅ Progress: Test now passes

...

SESSION COMPLETE
  Duration: 23m 42s
  Missions: 8 completed, 0 failed, 0 rejected
  Spec issues: 12 → 0
  Binding issues: 35 → 0  
  SUT issues: 3 → 0
  Coverage: 67% → 100%
  Token cost: ~$45
```

---

## Part 9: Key Insights

### 1. The Problem Was Measurement, Not Judgment

The primary issue wasn't that we needed LLM judges — it was that we were measuring progress at the wrong time (after scenarios added, before bindings created).

**Fix:** Chain missions and measure after the compound operation completes.

### 2. Deterministic Gates Beat LLM Gates

For rule-count invariant, we don't need an LLM to "judge" whether new rules were added. We can count them.

**Principle:** Use deterministic checks for deterministic properties.

### 3. LLM Judgment is for Edge Cases

The 3-judge consensus approach is valid and research-backed, but it's overkill for most cases. The simple "2+ scenarios per rule" heuristic handles 90% of coverage decisions.

**Use LLM assessment when:**
- Heuristic is ambiguous (rules with exactly 1 or 5+ scenarios)
- You want to validate scenario quality (are these the RIGHT tests?)
- You're assessing a completed feature before sign-off

### 4. Simplicity Wins

The best solution is:
1. **Mission chaining** (AddScenarios → CreateBindings as one unit)
2. **Rule-count invariant** (deterministic gate)
3. **2-scenario minimum** (simple heuristic)
4. **3-judge consensus** (only for ambiguous cases)

This is simple, elegant, research-backed, and fully autonomous.

---

## References

- **LLM-as-Judge Research:**
  - [Replacing Judges with Juries](https://arxiv.org/abs/2404.18796) (2024) — Diverse panels outperform single judges
  - [Self-Consistency Improves Chain of Thought](https://research.google/pubs/self-consistency-improves-chain-of-thought-reasoning-in-language-models/) — Same model, multiple samples
  - [Rulers: Locked Rubrics](https://arxiv.org/html/2601.08654v1) — Prevent evaluation drift
  
- **BDD Best Practices:**
  - Gáspár Nagy: [Divide & Conquer à la BDD](https://gasparnagy.com/2019/05/divide-conquer-a-la-bdd-story-rule-scenario/)
  - [Comprehensive Evaluation of LLMs for BDD](https://arxiv.org/pdf/2403.14965) (2024)
  
- **Traceability:**
  - [NASA AI-Enhanced Requirements Traceability](https://ntrs.nasa.gov/api/citations/20250008721/downloads/AI-Enhanced%20Requirements%20Traceability.pdf)

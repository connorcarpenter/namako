# Tesaki/Namako v1.9 Implementation Plan

**Created:** 2026-02-02
**Version:** 1.9
**Goal:** Bulletproof, economically optimized autonomous spec-driven development pipeline
**Priority:** Toolchain improvements first → enables better Naia development
**Philosophy:** Balanced token economy, aggressive momentum, high-quality output

---

## Executive Summary

Tesaki v1.8 works. Now we make it **excellent**.

This plan focuses on:
1. **Token Economy with Feedback** — Know what you're spending, optimize intelligently
2. **Model Tiering** — Right model for each job (research-backed)
3. **Quality over Quantity** — Better tokens, not fewer tokens
4. **System Reliability** — Handle edge cases gracefully
5. **Aggressive Momentum** — Keep the loop moving

---

## Part 1: Token Economy with Continuous Feedback

### 1.1 Token Usage Reporting (FOUNDATION)

**Rationale:** You can't optimize what you don't measure. Token usage reporting is the feedback signal that keeps paying dividends.

#### Implementation

- [ ] **Parse runner stderr for token usage** (Copilot CLI already emits this)
  ```
  Breakdown by AI model:
   claude-opus-4.5   4.8m in, 21.8k out, 935.5k cached (Est. 3 Premium requests)
  ```
- [ ] **Store token usage per mission** in `RUNNER_OUTPUT/token_usage.json`
  ```json
  {
    "model": "claude-opus-4.5",
    "tokens_in": 4800000,
    "tokens_out": 21800,
    "tokens_cached": 935500,
    "premium_requests": 3,
    "elapsed_seconds": 465
  }
  ```
- [ ] **Aggregate token usage per session** in `.tesaki/session_stats.json`
- [ ] **Display in console output** after each mission:
  ```
  📊 Tokens: 4.8M in, 21.8k out (935.5k cached) | Model: opus | Cost: ~$72
  ```
- [ ] **Add `tesaki stats` command** showing:
  - Total tokens used this session
  - Average tokens per mission type
  - Cost estimates by model
  - Comparison to previous sessions

**Files:** `tesaki/src/base_runner.rs`, `tesaki/src/repl.rs`, new `tesaki/src/stats.rs`

### 1.2 Token Usage Dashboard (Session End Summary)

At end of `tesaki --loop N`, display:
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
SESSION SUMMARY
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Missions:     8 completed, 0 failed
Duration:     23m 42s
Issues:       35 → 0 (-35)

Token Usage by Mission Type:
  CreateMissingBindings (3):    12.4M in, 58k out | ~$180
  FixRegression (2):             2.1M in, 14k out | ~$32
  AddOrClarifyScenario (3):      8.2M in, 42k out | ~$125

Total: 22.7M tokens in, 114k out
Estimated cost: ~$337
Avg cost per issue resolved: ~$9.63
```

---

## Part 2: Intelligent Model Tiering

### 2.1 Research-Backed Model Recommendations

Based on research into Claude Opus/Sonnet/Haiku capabilities and GPT-4/o1/o3 comparisons:

| Mission Type | Recommended Model | Rationale |
|-------------|-------------------|-----------|
| **AddOrClarifyScenario** | **Opus** | Requires deep understanding of domain, interpreting specs, writing high-quality BDD Gherkin. This is "spec interpretation" — needs highest intelligence. |
| **RefineFeatureIntent** | **Opus** | Understanding feature intent requires nuanced reasoning about requirements. |
| **CreateMissingBindings** | **Sonnet** | Pattern matching against existing bindings. Once you've seen one binding, you've seen them all. Structured, repetitive work. |
| **ImplementBehaviorForScenario** | **Sonnet → Opus** | Start with Sonnet. If it fails once, retry with Opus. Implementation is pattern-matching until it isn't. |
| **FixRegressionFromGateFailure** | **Opus** | Debugging requires deep reasoning about system behavior, understanding failure modes, tracing through code. Research confirms Opus excels here. |
| **NormalizeIdentityTags** | **Haiku** | Trivial syntactic transformation. Just adding `@Rule(nn)` tags. |
| **StrengthenThenAssertions** | **Sonnet** | Improving assertions is structured work with clear patterns. |
| **RefactorBindingsForClarity** | **Sonnet** | Code refactoring with known patterns. |
| **SummarizeAndClose** | **Haiku** | Summary generation is low-stakes. |
| **CleanupAfterSuccess** | **Haiku** | Housekeeping tasks. |

### 2.2 Implementation: Model Field per Mission Type

```rust
impl MissionType {
    /// Returns the recommended model for this mission type.
    pub fn recommended_model(&self) -> &'static str {
        match self {
            // High intelligence required
            Self::AddOrClarifyScenario { .. } => "opus",
            Self::RefineFeatureIntent { .. } => "opus",
            Self::FixRegressionFromGateFailure { .. } => "opus",
            
            // Structured work, patterns exist
            Self::CreateMissingBindings { .. } => "sonnet",
            Self::ImplementBehaviorForScenario { .. } => "sonnet",
            Self::StrengthenThenAssertions { .. } => "sonnet",
            Self::RefactorBindingsForClarity { .. } => "sonnet",
            
            // Trivial tasks
            Self::NormalizeIdentityTags { .. } => "haiku",
            Self::SummarizeAndClose => "haiku",
            Self::CleanupAfterSuccess => "haiku",
            
            // Meta (no runner)
            Self::ExplainState => "haiku",
            Self::TriageFailures => "sonnet",
        }
    }
}
```

**Files:** `tesaki/src/mission_type.rs`

### 2.3 Escalation on Failure

- [ ] Track model used per attempt
- [ ] On first failure with Sonnet/Haiku, escalate to next tier:
  - Haiku → Sonnet → Opus
- [ ] Log escalation in mission bundle
- [ ] Include in token usage reporting

```rust
fn select_model_for_attempt(mission_type: &MissionType, attempt: u32, prev_failure: bool) -> &'static str {
    let base = mission_type.recommended_model();
    if attempt == 1 || !prev_failure {
        return base;
    }
    // Escalate on retry
    match base {
        "haiku" => "sonnet",
        "sonnet" => "opus",
        "opus" => "opus", // Already at max
        _ => base,
    }
}
```

**Files:** `tesaki/src/main.rs`, `tesaki/src/repl.rs`

### 2.4 Config Override

Allow per-project model preferences in `.tesaki/config.toml`:

```toml
[model_overrides]
# Override default model for specific mission types
CreateMissingBindings = "opus"  # If your bindings are complex
AddOrClarifyScenario = "sonnet" # If your specs are simple

[model_defaults]
# Global default (overrides mission type recommendations)
default = "sonnet"

# Disable model tiering entirely
force_model = "opus"  # Use opus for everything
```

**Files:** `tesaki/src/config.rs`

---

## Part 3: Quality Over Quantity in Context

### 3.1 Philosophy: Better Tokens, Not Fewer Tokens

**The wrong approach:** Slash templates to save tokens, hope the runner figures it out.

**The right approach:** Every token should carry high information density. Remove fluff, keep signal.

### 3.2 Template Quality Audit

Review each template section and ask: "Does this token carry unique, actionable information?"

#### MISSION.md Analysis

| Section | Keep/Cut | Rationale |
|---------|----------|-----------|
| Mission ID, Type, Target | ✅ Keep | Essential identification |
| 🎯 Objective | ✅ Keep | Core purpose |
| Surface Policy table | ✅ Keep | Critical constraint |
| Surface lock warnings | ✅ Keep | Prevents violations |
| Context (missing steps list) | ✅ Keep | **This is the payload** |
| Static binding examples | ❓ Rethink | Runner can read codebase |
| Context types reference | ❌ Cut | Available in world.rs |
| Validation Criteria | ✅ Keep | Success definition |
| Budgets table | ✅ Keep | Constraints |
| How to Verify | ✅ Keep (slim) | One command, not explanation |

#### 3.3 Smart Context Injection

Instead of static examples, provide **dynamic, relevant context**:

- [ ] **For CreateMissingBindings:** Find the 2-3 most similar existing bindings (by step text similarity) and include those as examples.
- [ ] **For ImplementBehaviorForScenario:** Include the specific failure message and stack trace, not generic instructions.
- [ ] **For AddOrClarifyScenario:** Include the rule's existing scenarios as examples of the pattern.

This is **quality** context — specific to this mission, not boilerplate.

**Files:** `tesaki/src/prompts.rs`, templates

### 3.4 Revised Template Structure

```markdown
# Mission {{ mission_id }}

**Type:** {{ mission_type }} | **Model:** {{ model }} | **Target:** {{ target }}

## 🎯 Objective
{{ objective }}

## Surface Policy
| Surface | Policy | Paths |
|---------|--------|-------|
{{ surface_table }}

{% if surface_warnings %}
⚠️ {{ surface_warnings }}
{% endif %}

## Context
{{ dynamic_context }}

{% if similar_examples %}
## Similar Examples (from this codebase)
{{ similar_examples }}
{% endif %}

## Verify
```bash
{{ verify_command }}
```

---
*Tesaki v{{ version }} | {{ model }}*
```

**Target:** ~400 tokens base + dynamic context (~100-500 tokens depending on complexity)

---

## Part 4: System Reliability

### 4.1 Pre-Gate Compilation Check

**Problem:** Running `namako gate` on code that doesn't compile wastes time and tokens.

**Solution:**
- [ ] Before invoking gate, run `cargo build -p naia-tests --message-format=short`
- [ ] If build fails, parse errors and include in retry context
- [ ] Don't run gate until code compiles

```rust
fn pre_gate_check(spec_root: &Path) -> Result<(), Vec<CompileError>> {
    let output = Command::new("cargo")
        .args(["build", "-p", "naia-tests", "--message-format=short"])
        .current_dir(spec_root.parent().unwrap())
        .output()?;
    
    if output.status.success() {
        Ok(())
    } else {
        Err(parse_cargo_errors(&output.stderr))
    }
}
```

**Files:** `tesaki/src/main.rs`

### 4.2 Structured Error Parsing

- [ ] Parse `cargo build` errors into structured format
- [ ] Parse `namako lint` unresolved steps into structured format
- [ ] Include **top 5 errors only** in retry context (not all)
- [ ] Link errors to specific files/lines

```json
{
  "compile_errors": [
    {"file": "test/tests/src/steps/messaging.rs", "line": 42, "message": "cannot find value `ctx` in this scope"},
    {"file": "test/tests/src/steps/messaging.rs", "line": 58, "message": "mismatched types"}
  ],
  "lint_errors": [
    {"step": "Given a client is connected", "file": "03_messaging.feature", "line": 15}
  ]
}
```

**Files:** `tesaki/src/gate.rs`, new error parsing module

### 4.3 Graceful Failure Handling

- [ ] Catch runner process crashes (exit without output)
- [ ] Catch namako command failures (adapter issues)
- [ ] Catch timeout without killing state
- [ ] Preserve full diagnostic state for debugging
- [ ] Add `tesaki diagnose <mission_id>` command for failure analysis

**Files:** `tesaki/src/base_runner.rs`

### 4.4 State Recovery

- [ ] On startup, scan `.tesaki/missions/` for incomplete bundles
- [ ] Offer: "Found incomplete mission 037. Resume or clean up?"
- [ ] Log all state transitions to `.tesaki/session.log`

**Files:** `tesaki/src/session.rs`

---

## Part 5: Aggressive Momentum

### 5.1 Smart Stall Detection

**Current:** Stop after 3 consecutive no-progress missions.

**Better:** Track *why* there's no progress:

| Stall Type | Action |
|------------|--------|
| Same error recurring | Skip this mission type, try next |
| Different errors each time | Keep trying (system is exploring) |
| No changes made | Runner confused, escalate model |
| Changes made but issues increased | Possible regression, log warning but continue |

- [ ] Track error signatures per mission
- [ ] Compare error signatures across attempts
- [ ] Implement stall type classification

**Files:** `tesaki/src/repl.rs`

### 5.2 Mission Type Skip

- [ ] If same mission type fails 2× consecutively with same error pattern:
  - Mark type as "temporarily skipped"
  - Try next mission type
  - Unblock after different type succeeds
- [ ] Track skip list per session
- [ ] Display skipped types in status

**Files:** `tesaki/src/session.rs`, `tesaki/src/repl.rs`

### 5.3 Parallel-Ready Architecture (Future)

Structure code to allow future parallel execution:
- [ ] Identify independent missions (e.g., bindings for different features)
- [ ] Add `--parallel` flag (default off)
- [ ] Document parallelization constraints

---

## Part 6: Developer Experience

### 6.1 Enhanced Console Output

Already done:
- [x] Mission-specific success messages (📝, 🔧, 📋 emojis)
- [x] Cascade awareness messaging

Still needed:
- [ ] Token usage per mission (see Part 1)
- [ ] Progress bar for long-running missions (optional)
- [ ] ETA based on historical durations (optional)

### 6.2 Mission History

- [ ] Add `tesaki history` command
- [ ] Show recent missions with: type, duration, outcome, tokens, cost
- [ ] Persist in `.tesaki/history.jsonl`

### 6.3 Debug Mode

- [ ] Add `--debug` flag:
  - Preserve all mission bundles (not just failed)
  - Full namako command logging
  - Token usage details
- [ ] Add `tesaki explain-failure <mission_id>`:
  - Show mission context
  - Show runner output
  - Show gate errors
  - Suggest fixes

---

## Part 7: Implementation Sprints

### Sprint 1: Token Feedback (Foundation) — 3 items
1. [ ] 1.1 Parse and store token usage from runner stderr
2. [ ] 1.1 Display token usage in console after each mission
3. [ ] 1.2 Session end summary with token breakdown

### Sprint 2: Model Tiering — 4 items
4. [ ] 2.2 Add `recommended_model()` to MissionType
5. [ ] 2.3 Implement model escalation on failure
6. [ ] 2.4 Config override for model preferences
7. [ ] Wire model selection into runner invocation

### Sprint 3: Quality Context — 3 items
8. [ ] 3.3 Smart context injection (similar bindings, specific errors)
9. [ ] 3.4 Revise templates for quality over quantity
10. [ ] Measure token usage before/after (A/B comparison)

### Sprint 4: System Reliability — 4 items
11. [ ] 4.1 Pre-gate compilation check
12. [ ] 4.2 Structured error parsing
13. [ ] 4.3 Graceful failure handling
14. [ ] 4.4 State recovery on startup

### Sprint 5: Momentum & Polish — 4 items
15. [ ] 5.1 Smart stall detection (error signature tracking)
16. [ ] 5.2 Mission type skip on repeated failure
17. [ ] 6.2 Mission history command
18. [ ] 6.3 Debug mode and explain-failure command

---

## Part 8: Success Metrics

| Metric | Current | Target | How to Measure |
|--------|---------|--------|----------------|
| Token visibility | 0% | 100% | Token usage shown after every mission |
| Cost per binding | ~$8 | ~$3 | Token tracking |
| Cost per SUT fix | ~$72 | ~$40 | Token tracking |
| Build failures before gate | ~20% | 0% | Pre-gate check |
| Stall rate | ~10% | <3% | Smart stall detection |
| Recovery from crashes | 0% | 100% | State recovery |
| Model tier compliance | 0% | 100% | Right model for each job |

---

## Part 9: Critical Thinking Notes

### Why NOT to Over-Slim Templates

The research says "trust the runner" but context matters:

1. **The runner isn't free.** Every file it reads costs tokens. If we can provide the exact 3 similar bindings in 200 tokens, that's cheaper than the runner grepping through 5000 lines of code.

2. **Quality context beats no context.** A sparse template that says "figure it out" leads to the runner exploring, making mistakes, retrying. A rich template with relevant examples leads to first-attempt success.

3. **The right metric is cost per success, not cost per token.** A 2000-token mission that succeeds on first try is cheaper than a 500-token mission that needs 5 retries.

### Why Model Tiering Matters

The research is clear:
- **Opus:** Deep reasoning, debugging, complex interpretation. ~$75/1M tokens.
- **Sonnet:** Structured work, patterns, implementation. ~$15/1M tokens.
- **Haiku:** Trivial transformations. ~$1.25/1M tokens.

Using Opus for everything is 5-60× more expensive than necessary for routine work.

But using Haiku for spec interpretation is penny-wise, pound-foolish — low-quality specs cascade into low-quality tests into low-quality implementations.

### Why Token Feedback is Non-Negotiable

Without token feedback:
- You don't know what optimizations are working
- You can't compare model performance
- You can't make data-driven decisions

With token feedback:
- You see exactly where tokens go
- You can A/B test template changes
- You can justify model choices with data

---

## Part 10: Non-Goals (Explicitly Excluded)

1. **CI/CD integration** — Not needed per user preference
2. **Multi-language support** — Deferred to v2
3. **Rollback on regression** — User prefers aggressive momentum
4. **Parallel execution** — Prepare architecture, but don't implement yet
5. **End-to-end validation suites** — Trust incremental testing

---

## Part 11: Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Model tiering reduces quality | Track success rate per model, revert if degraded |
| Template changes break runner | A/B test with token tracking |
| Token parsing breaks with runner updates | Graceful fallback if parsing fails |
| Smart stall detection is too aggressive | Conservative defaults, tune based on data |

---

*Plan version: 1.9*
*Author: Copilot CLI with human guidance*
*Last updated: 2026-02-02*

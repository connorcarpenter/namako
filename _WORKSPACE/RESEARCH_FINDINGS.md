# Research Findings: Autonomous Coding Loops & Spec-Driven Development

## Sources
- [Addy Osmani - Self-Improving Coding Agents](https://addyosmani.com/blog/self-improving-agents/)
- [OpenAI - Unrolling the Codex Agent Loop](https://openai.com/index/unrolling-the-codex-agent-loop/)
- [Yohei Nakajima - Better Self-Improving AI Agents](https://yoheinakajima.com/better-ways-to-build-self-improving-ai-agents/)
- [arXiv - A Self-Improving Coding Agent](https://arxiv.org/html/2504.15228v2)
- [Cucumber BDD Docs](https://cucumber.io/docs/bdd/)

---

## Key Principles for "Ralph Wiggum" Loops

### 1. Atomic Task Decomposition
**Each task must be small, clear, and objectively verifiable.**

- Fits in context window
- Has clear success criteria (test passes, lint clean)
- Single responsibility

### 2. Stateless Iteration
**Reset context between tasks to avoid drift.**

```
for task in work_queue:
    fresh_state = compute_state()  # No memory of previous attempts
    result = execute(task, fresh_state)
    if result.success:
        commit()
    else:
        log_failure()
```

### 3. Continuous Validation
**Every code change is immediately validated.**

- Run tests after every modification
- Feedback loop closes in seconds, not hours
- Failed validation = retry with feedback

### 4. Self-Reflection on Failure
**When something fails, capture WHY and feed it back.**

```
if lint_failed:
    error_message = parse_lint_output()
    next_prompt += f"Previous attempt failed: {error_message}"
```

### 5. Rollback on Regression
**If output is worse than input, revert immediately.**

```
if after_issues > before_issues:
    git_reset()
    log("Regression detected, reverted")
```

---

## BDD Best Practices

### The Discovery→Formulation→Automation Flow

1. **Discovery**: Understand what behavior is needed (the spec)
2. **Formulation**: Write it in Gherkin (Given/When/Then)
3. **Automation**: Implement step bindings, then implement SUT

### Gherkin Anti-Patterns to Avoid

- ❌ Implementation details in scenarios ("click button", "query database")
- ❌ Multiple behaviors per scenario
- ❌ Vague assertions ("it works correctly")

### Gherkin Best Practices

- ✅ One behavior per scenario
- ✅ Business language, not technical
- ✅ Concrete examples with specific values

---

## Applying to Tesaki/Namako

### What We're Doing Right

| Practice | Status |
|----------|--------|
| Atomic tasks (one mission type) | ✅ |
| Stateless (recompute RepoState each cycle) | ✅ |
| Validation (namako gate) | ✅ |
| Clear success criteria | ✅ |

### What We're Missing

| Practice | Status | Priority |
|----------|--------|----------|
| Failure feedback in next prompt | ❌ | HIGH |
| Rollback on regression | ❌ | MEDIUM |
| Self-reflection log | ❌ | LOW |

---

## Recommended Minimal Changes

### 1. Failure Feedback (HIGH)
When gate fails, include the error in the next mission context:
```
Previous attempt failed: MissingStep { step_text: "Given a client" }
```

### 2. Rollback on Regression (MEDIUM)
```rust
if after_total > before_total + 5 {  // Significant regression
    git_checkout_all();
    skip_this_mission_type();
}
```

### 3. Don't Overengineer
The research is clear: **simple, focused loops beat complex orchestration.**

- ❌ Don't add exemplar extraction if the runner can find examples itself
- ❌ Don't add contract snippet injection if the feature file header is readable
- ✅ Trust the runner to read files and figure out patterns
- ✅ Keep the loop simple: select → execute → validate → repeat

---

## The Minimal "Ralph Wiggum" Formula

```
while has_work() and not stalled:
    state = compute_fresh_state()
    mission = select_deterministically(state)
    
    before = count_issues(state)
    execute(mission)
    after = count_issues(refresh_state())
    
    if after < before:
        continue  # Progress!
    elif after == before:
        stall_count++
    else:
        rollback()  # Regression
```

That's it. No exemplars, no contract snippets, no self-reflection logs. Just:
1. Pick the next task
2. Do it
3. Check if it helped
4. Repeat

The runner (Copilot/Claude) is smart enough to figure out patterns from the codebase. We don't need to spoon-feed it.

---

## Efficiency Improvement: Headless Mode

**Problem:** User had to start REPL, then type `loop N`

**Solution:** Added `tesaki --loop N` CLI flag for headless autonomous mode

```bash
# Before (3 steps)
$ tesaki
> loop 10
...watch output...

# After (1 step)
$ tesaki --loop 10
```

This is the "turnkey" interface the research recommended.

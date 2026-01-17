# Naia Spec Conversion Plan

This document tracks the conversion of 15 markdown contract specifications into executable Gherkin `.feature` files using the Namako pipeline.

## Conversion Rules

### Strict Slice Workflow
For each slice (1–3 scenarios):
1. **Convert** — Write the `.feature` scenarios
2. **Lint** — `namako lint` must be green
3. **Run** — `namako run` must be green
4. **Verify** — `namako verify` must be green
5. **Only then** proceed to next slice

Only run `update-cert` when explicitly choosing to lock a baseline for a milestone.

### Conventions
- One `.feature` file per conceptual area initially
- Use comments (`# ...`) for rationale in step bindings
- Keep scenario names stable once certified
- Avoid reformatting large blocks after certification (line-number keys drift)

### Binding Strategy
- Prefer reusing existing bindings; don't explode the binding set
- When adding bindings, keep expressions narrow to avoid ambiguity
- Fix ambiguity by tightening expressions, not by hacks

### Real Behavior First
Every slice should touch real Naia behavior. Avoid pure toy steps — they don't build confidence.

---

## Contract Documents

| # | Contract | Slices | Status |
|---|----------|--------|--------|
| 00 | [common](contracts/00_common.spec.md) | N/A | Shared definitions only |
| 01 | [connection_lifecycle](contracts/01_connection_lifecycle.spec.md) | 3 | **Slice 1 In Progress** |
| 02 | [transport](contracts/02_transport.spec.md) | TBD | Not started |
| 03 | [messaging](contracts/03_messaging.spec.md) | TBD | Not started |
| 04 | [time_ticks_commands](contracts/04_time_ticks_commands.spec.md) | TBD | Not started |
| 05 | [observability_metrics](contracts/05_observability_metrics.spec.md) | TBD | Not started |
| 06 | [entity_scopes](contracts/06_entity_scopes.spec.md) | TBD | Not started |
| 07 | [entity_replication](contracts/07_entity_replication.spec.md) | TBD | Not started |
| 08 | [entity_ownership](contracts/08_entity_ownership.spec.md) | TBD | Not started |
| 09 | [entity_publication](contracts/09_entity_publication.spec.md) | TBD | Not started |
| 10 | [entity_delegation](contracts/10_entity_delegation.spec.md) | TBD | Not started |
| 11 | [entity_authority](contracts/11_entity_authority.spec.md) | TBD | Not started |
| 12 | [server_events_api](contracts/12_server_events_api.spec.md) | TBD | Not started |
| 13 | [client_events_api](contracts/13_client_events_api.spec.md) | TBD | Not started |
| 14 | [world_integration](contracts/14_world_integration.spec.md) | TBD | Not started |

---

## Slice Breakdown (to be filled per contract)

### 00_common
_Prerequisites and shared definitions — likely no scenarios, just background context_

### 01_connection_lifecycle

**Status: IN PROGRESS**

**Slice Breakdown:**

| Slice | Scenarios | Obligations Covered | Status |
|-------|-----------|---------------------|--------|
| 1 | Event ordering (auth mode), Event ordering (client), Rejection emits RejectEvent only | connection-24, connection-25, connection-26, connection-19, connection-21, connection-22 | ✅ Lint+Run pass, awaiting update-cert |
| 2 | Reconnect is fresh session, Entity despawn on disconnect | connection-28.t1, connection-28.t2, connection-23.t1 | Not Started |
| 3 | Protocol identity mismatch rejection | connection-31.t1, connection-31.t2, connection-31.t3 | Not Started |

**Rationale for starting here:**
- The smoke test already exercises basic connect/disconnect flows
- The harness (`naia_test_harness::Scenario`) already has auth/connect/disconnect patterns
- These are foundational behaviors everything else depends on

### 02_transport
_TBD: UDP/WebRTC transport layer behaviors_

### 03_messaging
_TBD: Message reliability, ordering, channels_

### 04_time_ticks_commands
_TBD: Tick synchronization, command queuing_

### 05_observability_metrics
_TBD: Metric emission, counters, gauges_

### 06_entity_scopes
_TBD: Scope definitions, visibility rules_

### 07_entity_replication
_TBD: Replicate behavior, property sync_

### 08_entity_ownership
_TBD: Ownership rules, owner changes_

### 09_entity_publication
_TBD: Publish/unpublish lifecycle_

### 10_entity_delegation
_TBD: Delegation mechanics_

### 11_entity_authority
_TBD: Authority model, handoffs_

### 12_server_events_api
_TBD: Server-side event hooks_

### 13_client_events_api
_TBD: Client-side event hooks_

### 14_world_integration
_TBD: Integration with external world/ECS systems_

---

## Progress Log

| Date | Contract | Slice | Notes |
|------|----------|-------|-------|
| 2026-01-16 | (smoke) | — | `namako_smoke.feature` created (2 scenarios), pipeline proven |
| 2026-01-16 | 01_connection_lifecycle | 1 | `connection_lifecycle.feature` created (3 scenarios), lint+run pass, baseline stale |

---

## Current State

**Completed:**
- Smoke test: `features/smoke/namako_smoke.feature` (2 scenarios)
- Connection Lifecycle Slice 1: `features/connection/connection_lifecycle.feature` (3 scenarios)
- Pipeline proven: lint → run → (verify requires update-cert)

**Blocking:**
- Baseline certification is stale — need to run `namako update-cert` after confirming scenarios are correct

**Next:**
- Run `namako update-cert` to lock baseline
- Confirm `namako verify` passes
- Continue with `01_connection_lifecycle` Slice 2

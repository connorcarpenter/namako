# Tesaki Prompt Templates

This directory contains all prompt templates used by Tesaki to generate mission documents for runners.

## Directory Structure

```
prompts/
├── README.md                     # This file
├── mission/
│   ├── MISSION.md.j2             # Main mission prompt
│   ├── POLICY.md.j2              # Policy rules
│   └── briefs/                   # Per-mission-type briefs
│       ├── create_missing_bindings.md.j2
│       ├── implement_behavior.md.j2
│       ├── fix_regression.md.j2
│       ├── refine_feature_intent.md.j2
│       ├── add_clarify_scenario.md.j2
│       ├── normalize_identity_tags.md.j2
│       ├── strengthen_then.md.j2
│       ├── refactor_bindings.md.j2
│       ├── summarize_and_close.md.j2
│       ├── cleanup_after_success.md.j2
│       ├── explain_state.md.j2
│       └── triage_failures.md.j2
├── next_task/
│   ├── base.md.j2                # Common header/footer
│   ├── done.md.j2                # DONE action variant
│   ├── fix_lint.md.j2            # FIX_LINT action variant
│   ├── fix_run.md.j2             # FIX_RUN action variant
│   ├── needs_approval.md.j2      # NEEDS_UPDATE_CERT_APPROVAL
│   ├── run_gate.md.j2            # RUN_LINT/RUN/RUN_VERIFY
│   └── unknown.md.j2             # Fallback
└── components/
    ├── surfaces_table.md.j2      # Surface policy table
    ├── budgets_table.md.j2       # Budget limits table
    └── footer.md.j2              # Common footer
```

## Template Syntax

Templates use [MiniJinja](https://github.com/mitsuhiko/minijinja) syntax, which is compatible with Jinja2.

### Variables

Use `{{ variable }}` to insert a variable value:

```jinja2
# Mission {{ mission_id }}
**Type:** {{ mission_type }}
```

### Conditionals

Use `{% if %}` blocks for conditional content:

```jinja2
{% if target %}
**Target:** {{ target }}
{% endif %}
```

### Loops

Use `{% for %}` blocks for iteration:

```jinja2
{% for criterion in validation_criteria %}
{{ loop.index }}. {{ criterion }}
{% endfor %}
```

### Includes

Use `{% include %}` to embed other templates:

```jinja2
{% include "components/surfaces_table.md.j2" %}
```

### Filters

Common filters:
- `{{ value | upper }}` - Convert to uppercase
- `{{ list | join(", ") }}` - Join list elements
- `{{ value | default("fallback") }}` - Provide default value

## Adding New Templates

1. Create the `.md.j2` file in the appropriate directory
2. Define the required context variables in a comment at the top
3. Add the template to `prompts.rs` in the `create_environment()` function
4. Define the context struct if needed
5. Add tests for the new template

## Context Variables

### Mission Templates

| Variable | Type | Description |
|----------|------|-------------|
| `mission_id` | String | Mission identifier (e.g., "001-create-abc12345") |
| `mission_type` | String | Mission type name |
| `stage` | String | Current stage name |
| `target` | Option<String> | Target label (scenario key, feature path) |
| `objective` | String | Mission objective |
| `context` | String | Mission context |
| `validation_criteria` | Vec<String> | List of validation criteria |
| `surface_policy` | Object | Spec/tests/sut lock status |
| `surface_definitions` | Object | Path patterns for each surface |
| `budgets` | Object | Budget limits |
| `version` | String | Tesaki version |

### Next Task Templates

| Variable | Type | Description |
|----------|------|-------------|
| `timestamp` | String | Generation timestamp |
| `action` | String | Recommended action |
| `status` | Object | Status JSON parsed object |
| `review` | Object | Review JSON parsed object |
| `eligible_candidates` | Vec | Filtered promotion candidates |
| `update_cert_message` | Option<String> | Update cert message if applicable |
| `max_cert_updates` | u32 | Max autonomous cert updates |

## Testing Templates

Run template tests with:

```bash
cargo test -p tesaki template
```

All templates are validated at compile time to ensure:
- Syntax is valid
- Required includes exist
- Basic rendering works

## Best Practices

1. **Keep templates focused** - Each template should do one thing well
2. **Use includes for reuse** - Don't duplicate table structures
3. **Document context variables** - Future maintainers need to know what's available
4. **Test edge cases** - Empty lists, missing optionals, long strings
5. **Match output format** - Templates generate Markdown, so format accordingly

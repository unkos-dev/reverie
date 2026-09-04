---
title: Reference
description: Generated configuration and API reference for a Reverie instance.
---

The Reference section is generated from source, not written by hand. Two
artifacts feed it:

- **[Configuration](/reverie/reference/configuration/)**: rendered from the
  backend's declarative configuration schema (`schemars`) joined with the
  operator-facing environment-variable map. Every variable, type, default, and
  required flag comes straight from the code that reads it. Secret-bearing
  variables are listed by name only; their values never appear.
- **API Reference**: rendered by `starlight-openapi` from the committed
  OpenAPI 3.1 spec (`backend/openapi.json`), itself generated code-first from the
  annotated route handlers.

## Regenerating

Generated pages are not edited directly. Both artifacts are drift-gated by
backend tests that fail CI when the committed output goes stale. To regenerate
after a config or handler change:

```bash
cd backend
REGEN=1 cargo test --test gen_openapi --test gen_config_ref
```

Then commit the updated `configuration.mdx` and `openapi.json`. This is the
docs-as-done mechanism; reference docs ship with the change that alters them,
the same way the TDD mandate keeps code tested.

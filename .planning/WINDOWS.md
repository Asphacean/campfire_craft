---
schema_version: 1
open_count: 1
waived_count: 0
fixed_count: 0
total_count: 1
last_updated: 2026-08-27T14:16:06.134Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 01 | unrun-verify | scripts/restore.sh |  | restore.sh's 'stop-fails, nothing touched' acceptance criterion was not exercised live (too risky to force on the single live production instance) — the refusal logic exists (systemctl is-active check after stop) but was proven only via a fully valid stop path | open |  | 2026-08-27T14:16:06.134Z |  |

````json
[
  {
    "id": 1,
    "kind": "unrun-verify",
    "phase": "01",
    "file": "scripts/restore.sh",
    "line": null,
    "description": "restore.sh's 'stop-fails, nothing touched' acceptance criterion was not exercised live (too risky to force on the single live production instance) — the refusal logic exists (systemctl is-active check after stop) but was proven only via a fully valid stop path",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-08-27T14:16:06.134Z",
    "resolved_at": null
  }
]
````

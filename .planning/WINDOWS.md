---
schema_version: 1
open_count: 6
waived_count: 0
fixed_count: 0
total_count: 6
last_updated: 2026-08-30T14:54:24.378Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 01 | unrun-verify | scripts/restore.sh |  | restore.sh's 'stop-fails, nothing touched' acceptance criterion was not exercised live (too risky to force on the single live production instance) — the refusal logic exists (systemctl is-active check after stop) but was proven only via a fully valid stop path | open |  | 2026-08-27T14:16:06.134Z |  |
| 2 | 02 | deviation | scripts/auth-smoke.sh |  | Task 2 (tdd=true): test-extension and implementation were written together rather than as a strict two-commit RED-then-GREEN sequence; RED was verified retroactively by building the Task-1 commit (36c7084) via git archive into a scratch dir and confirming the new assertions (invalid nick, weak password, missing-field 400, flood 429, /status, CLI login) genuinely failed against it before the Task-2 commit landed. | open |  | 2026-08-28T10:56:15.641Z |  |
| 3 | 03 | deviation | caddy/Caddyfile |  | Plan's plaintext-HTTP acceptance criterion (curl to :8444 without TLS must exit non-zero) does not hold on this Caddy/Go stack: Go's net/http replies with a benign HTTP 400 'Client sent an HTTP request to an HTTPS server' instead of dropping the connection, so curl exits 0. The actual security property (no content served in plaintext) holds; verified with curl -v. Literal criterion text does not match real tool behavior. | open |  | 2026-08-28T14:47:47.918Z |  |
| 4 | 04 | stub | launcher/ui/main.js |  | Open log button shows an alert() with the log path instead of revealing the file in the OS file manager (needs tauri-plugin-opener, deferred to wave 4 alongside Game folder) | open |  | 2026-08-28T17:52:34.052Z |  |
| 5 | 04 | unrun-verify | 04-01-PLAN.md |  | Task 3's <human-check> (build-from-source on Windows x64, click through the window) was not performed on this Pi (no display) — pending per 04-01-SUMMARY.md 'Pending Human Verification' | open |  | 2026-08-28T17:52:34.242Z |  |
| 6 | 04 | unrun-verify | docs/LAUNCHER-BUILD.md |  | Phase 4 operator QA matrix (17 items, Windows x64 + Apple Silicon) not yet run on real hardware — this Pi has no display | open |  | 2026-08-30T14:54:24.378Z |  |

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
  },
  {
    "id": 2,
    "kind": "deviation",
    "phase": "02",
    "file": "scripts/auth-smoke.sh",
    "line": null,
    "description": "Task 2 (tdd=true): test-extension and implementation were written together rather than as a strict two-commit RED-then-GREEN sequence; RED was verified retroactively by building the Task-1 commit (36c7084) via git archive into a scratch dir and confirming the new assertions (invalid nick, weak password, missing-field 400, flood 429, /status, CLI login) genuinely failed against it before the Task-2 commit landed.",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-08-28T10:56:15.641Z",
    "resolved_at": null
  },
  {
    "id": 3,
    "kind": "deviation",
    "phase": "03",
    "file": "caddy/Caddyfile",
    "line": null,
    "description": "Plan's plaintext-HTTP acceptance criterion (curl to :8444 without TLS must exit non-zero) does not hold on this Caddy/Go stack: Go's net/http replies with a benign HTTP 400 'Client sent an HTTP request to an HTTPS server' instead of dropping the connection, so curl exits 0. The actual security property (no content served in plaintext) holds; verified with curl -v. Literal criterion text does not match real tool behavior.",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-08-28T14:47:47.918Z",
    "resolved_at": null
  },
  {
    "id": 4,
    "kind": "stub",
    "phase": "04",
    "file": "launcher/ui/main.js",
    "line": null,
    "description": "Open log button shows an alert() with the log path instead of revealing the file in the OS file manager (needs tauri-plugin-opener, deferred to wave 4 alongside Game folder)",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-08-28T17:52:34.052Z",
    "resolved_at": null
  },
  {
    "id": 5,
    "kind": "unrun-verify",
    "phase": "04",
    "file": "04-01-PLAN.md",
    "line": null,
    "description": "Task 3's <human-check> (build-from-source on Windows x64, click through the window) was not performed on this Pi (no display) — pending per 04-01-SUMMARY.md 'Pending Human Verification'",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-08-28T17:52:34.242Z",
    "resolved_at": null
  },
  {
    "id": 6,
    "kind": "unrun-verify",
    "phase": "04",
    "file": "docs/LAUNCHER-BUILD.md",
    "line": null,
    "description": "Phase 4 operator QA matrix (17 items, Windows x64 + Apple Silicon) not yet run on real hardware — this Pi has no display",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-08-30T14:54:24.378Z",
    "resolved_at": null
  }
]
````

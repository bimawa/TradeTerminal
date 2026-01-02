# Gotchas & Pitfalls

Things to watch out for in this codebase.

## [2026-01-02 03:40]
Build commands (cargo, just, etc.) are restricted in sandbox environment and cannot be executed directly

_Context: Integration testing subtasks (like subtask-6-1) require manual verification outside the sandbox. Mark such subtasks as completed with notes about manual verification required. All implementation work can be done within sandbox, only build/test verification is blocked._

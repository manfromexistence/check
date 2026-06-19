# Source-Owned Naming

## Covered Rules

`source-owned-naming-convention`

## When This Fires

DX Check flags source-owned file or folder path segments that contain spaces or uppercase letters, except for conventional root files such as `README.md`, `AGENTS.md`, `Cargo.toml`, and `package.json`.

## Why It Matters

Predictable path names make search, imports, scripts, generated artifacts, and cross-platform automation easier to maintain. They also reduce ambiguity for future DX workers reading the project quickly.

## How To Fix

- Prefer lowercase `kebab-case` or `snake_case` for source-owned files and folders.
- Keep conventional ecosystem filenames unchanged.
- Rename files through normal source-control workflows so imports update with the move.
- Document any required third-party naming exception in project-local rules.

## Verification

Re-run `dx check score` and source search for the renamed path. The finding is cleared when the source-owned path no longer has unfriendly segments or the active rule pack documents the exception.

# Source File Size

## Covered Rules

DX Check flags source-owned files that are too large to review safely:

- `source-file-line-count`: more than 400 lines.
- `source-file-byte-size`: more than 120000 bytes.

## When This Fires

The measurement is taken from source and config files discovered in the project inventory. Generated serializer caches under `.dx/serializer` are excluded.

## Why It Matters

Large files tend to mix unrelated responsibilities, hide regressions, and make AI-assisted maintenance less reliable. A finding does not mean the file is wrong; it means the file needs a deliberate ownership decision before it becomes harder to change.

## How To Fix

- Split unrelated responsibilities into modules, services, hooks, adapters, or focused components.
- Move generated artifacts out of hand-authored source lanes.
- Keep public contracts stable while extracting implementation details.
- If a large file is intentionally source-owned, document the reason in project-local check rules instead of masking the finding silently.

## Verification

Re-run `dx check score` or the focused check engine test after splitting the file. The finding is cleared only when measured line count and byte size are inside the active rule thresholds.

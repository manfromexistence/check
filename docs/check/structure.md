# AI-Maintainable Project Structure

## Covered Rules

`ai-maintainable-project-structure`

## When This Fires

DX Check looks for at least one orientation file that helps future maintainers recover project intent:

- `README.md`
- `TODO.md`
- `CHANGELOG.md`
- `AGENTS.md`
- `DX.md`
- extensionless `dx`

## Why It Matters

AI-assisted development works best when the project preserves intent in source-owned files. Orientation files reduce rediscovery work, clarify ownership, and make future changes safer.

## How To Fix

- Add or maintain a concise `README.md`, `AGENTS.md`, or extensionless `dx` file.
- Keep current task status in `TODO.md` when that matches the repo convention.
- Record user-facing changes in `CHANGELOG.md` when releases matter.
- Prefer useful project context over process notes or generated filler.

## Verification

Re-run `dx check score`. The finding is cleared when at least one recognized orientation file is present in the scanned source-owned tree.

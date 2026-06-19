# Generated Artifact Ownership

## Covered Rules

DX Check flags generated artifacts that appear in hand-authored source lanes:

- `.machine` files outside `.dx/serializer`.
- Generated source filenames such as `.generated.ts`, `.gen.rs`, `.pb.go`, `.pb.ts`, and similar forms.
- Source files whose first 64 KiB contain generated markers such as `@generated`, `do not edit`, or `code generated`.

Documentation and serializer cache files under `.dx/serializer` are not treated as source leaks.

## Why It Matters

Generated files need a clear owner. Mixing generated output into hand-authored lanes makes reviews noisy, causes agents to edit the wrong layer, and can break regeneration workflows.

## How To Fix

- Put serializer machine output under `.dx/serializer`.
- Keep generator inputs and hand-authored source separate from generated output.
- Check in generated files only when the project has an explicit regeneration contract.
- Add source comments or project docs that point to the generator command when generated source must be committed.

## Verification

Re-run `dx check score` and inspect the finding file paths. The finding is cleared when generated machine artifacts live under `.dx/serializer` or generated source has a documented ownership path.

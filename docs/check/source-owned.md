# Source-Owned Dependency Boundaries

## Covered Rules

`source-owned-node-modules`

## When This Fires

DX Check flags `node_modules` when it appears inside the scanned source-owned project tree.

## Why It Matters

Dependency installs are large, generated, and machine-local. When they sit inside source-owned DX or Forge lanes, they distort scoring, slow scans, hide real source files, and can accidentally leak into artifacts or reviews.

## How To Fix

- Keep dependency installs in the normal package-manager location and out of committed source lanes.
- Do not copy `node_modules` into templates, examples, receipts, or rule packs.
- Add ignore rules or cleanup steps for generated dependency folders.
- If a vendored dependency is intentional, store it in an explicit vendor lane and document the exception.

## Verification

Re-run `dx check score` after cleanup. The finding is cleared when the scanned tree no longer contains a source-owned `node_modules` directory.

# Component Maintainability

## Covered Rules

DX Check evaluates component-shaped `.tsx` and `.jsx` files. A file is treated as component-shaped when it lives under a component path, contains `component` in the path, or uses a PascalCase file stem.

Default component rules flag:

- Components over 300 lines.
- Direct imports of server-only modules such as `fs`, `path`, `child_process`, `net`, `tls`, `http`, `https`, or `process`.
- Components without a `className`, interface, `React.ComponentProps`, or `*Props` affordance for design-system composition.

## Why It Matters

Healthy components are easy to compose, test, and move across app boundaries. These checks encourage shadcn-like component quality without requiring shadcn or depending on `node_modules`.

## How To Fix

- Extract data access, state orchestration, and presentation into focused files.
- Move server-only work behind server actions, route handlers, APIs, or adapter modules.
- Expose typed props and a `className` affordance where a component is meant to be composed.
- Keep exceptions explicit when a component is intentionally private and not reusable.

## Verification

Re-run `dx check score` after extraction. The finding is cleared when component files are under the active size threshold, no longer import server-only modules directly, and expose the expected composition affordance.

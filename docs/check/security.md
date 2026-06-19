# Secure Source Defaults

## Covered Rules

`insecure-source-defaults`

## When This Fires

DX Check flags source files that contain simple insecure default markers, including hard-coded password, secret, or API key string assignments and React raw-HTML rendering usage.

The check is intentionally conservative. It is a project-health signal, not a full security audit.

## Why It Matters

Hard-coded secrets, unsafe defaults, and unsafe HTML rendering can turn local convenience into production risk. These findings keep launch readiness honest by marking the evidence as measured without pretending to replace dedicated security review.

## How To Fix

- Move secrets into the project-approved environment or secret-management layer.
- Replace unsafe defaults with explicit configuration and safe fallbacks.
- Avoid raw HTML rendering; when it is required, sanitize input and document the trust boundary.
- Run the project security tooling before treating the launch receipt as production evidence.

## Verification

Re-run `dx check score` and the project security tooling. The finding is cleared when the insecure marker is removed or the project-local rule pack documents a reviewed exception.

# Semantic Versioning Policy for NCX Infra Controller

This document defines the semantic versioning (SemVer) policy for the **Carbide family of repositories**:
- `ncx-infra-controller-core` (Core)
- `ncx-infra-controller-rest` (Rest)

Both repos follow this policy independently. Their version numbers may diverge across releases — this is expected and acceptable.

## Version Format

```
MAJOR.MINOR.PATCH
```

| Component | Example | Meaning |
|-----------|---------|---------|
| `MAJOR`   | `2.0.0` | Backward-incompatible change |
| `MINOR`   | `1.3.0` | New functionality, backward-compatible |
| `PATCH`   | `1.3.1` | Bug fix or hotfix, backward-compatible |

---

## When to Increment Each Component

### MAJOR — Backward-Incompatible Changes

A major version bump is required when a release:

- **Drops support for a hardware model** — the top-of-tree software will no longer run on that hardware. The dropped hardware may receive an orphaned maintenance branch, but no engineering resources will be allocated to it unless mandated by company leadership due to business need (e.g., a significant installed base requiring critical bug fixes).
- **Introduces a backward-incompatible REST API change** — removes or modifies existing endpoints or fields in a breaking way.
- **Removes or breaks support for event bus messages** — changes to event bus message handling are treated as API incompatibilities.

> The decision to drop hardware support is not made solely by the engineering team. Company leadership may mandate continued support if there is significant business justification.

### MINOR — Regular Release Cadence

A minor version is incremented with each **scheduled release cycle**. The team follows a **four-week (28-day) release cadence**. Minor releases are time-based, not feature-gated — the minor version is bumped regardless of which specific features land in a given cycle.

This cadence ensures compliance with regulatory requirements (e.g., FedRAMP) that mandate release intervals shorter than 30 days.

### PATCH — Hotfixes and Bug Fixes

Patch versions are **not tied to the release schedule** and may be issued at any time — including same-day — to address:

- Urgent bugs discovered after a release
- Security fixes
- Compliance-driven corrections

---

## Backward Compatibility Guarantees

Within a given major version, the team guarantees:

1. **Hardware support is not dropped** — all hardware models supported at the start of the major version remain supported throughout.
2. **REST API compatibility is maintained** — no breaking changes to existing API endpoints, request/response schemas, or authentication contracts.
3. **Event bus message compatibility is maintained** — existing message types and formats on the event bus will not be removed or altered in an incompatible way.

---

## Release Cadence and Process

### Scheduled Minor Releases

1. At the end of each four-week cycle, a **release candidate (RC)** is tagged after development stabilizes.
2. A **release branch** is created from the RC tag to isolate the release from ongoing development on `main`.

### Pre-Release Tags

Pre-release and RC tags use short commit hashes to simplify scripting and tooling integration (e.g., SBOM generation):

```
v1.3.0-rc.<short-hash>
v1.3.0-pre.<short-hash>
```

### Hotfix / Patch Releases

Patch releases bypass the four-week schedule. They are applied directly to the relevant release branch and tagged immediately after any required approval, following the same cherry-pick-from-main workflow where applicable.

---

## Summary Table

| Change Type | Version Component | Scheduled? |
|---|---|---|
| Drop hardware support | MAJOR | No — triggered by incompatibility |
| Breaking REST API change | MAJOR | No — triggered by incompatibility |
| Breaking event bus change | MAJOR | No — triggered by incompatibility |
| Regular four-week release | MINOR | Yes — every 28 days |
| Bug fix / hotfix | PATCH | No — as needed |

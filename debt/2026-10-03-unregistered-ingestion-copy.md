---
severity: medium
surfaces: [developer, server-operator, end-user]
adopted: 2026-10-03
adopted-because: Ingestion publishes and validates the library copy before registering its manifestation and path claim.
lift-when-class: internal-refactor
lift-when: Independently owned staged validation and full ingestion outcome reconciliation prove that newly failed terminal attempts leave no unregistered library copy and never remove a committed owner's file.
---

# Ingestion can retain an unregistered library copy

Ingestion preserves copy, validate and insert ordering. Path selection checks claims, initial publication and metadata
moves refuse overwriting, and manifestation insertion commits its claim atomically. These controls do not give the
published copy a durable owner before registration.

Interruption before registration, uncertain publication or commit, and changed candidate identity can retain library
bytes without a manifestation. Protected cleanup retains and reports uncertain ownership rather than deleting a
committed owner's file. No orphan scavenging is provided.

Remove this entry when ingestion validates independently owned staging and reconciles input, attempt and manifestation
outcomes. Regression evidence must show that newly failed terminal attempts leave no unregistered library copy and never
remove a committed owner's file, including database failure and interrupted publication.

/**
 * Per-version metadata mutators for the book detail Versions tab.
 *
 * Wraps the existing
 * `POST /api/manifestations/{id}/metadata/{accept,reject,revert}`
 * endpoints (shipped in earlier sub-phases) with typed call sites and
 * the central `apiFetch` wrapper so the CSRF token + RFC 7807 parsing
 * apply uniformly.
 *
 * The accept/reject/revert URLs use `/api/manifestations/` (the legacy
 * shape) while the new manual-edit endpoint uses `/api/books/` (per
 * the JSON-API ADR). Both refer to the same `manifestations.id` UUID —
 * the URL prefix is a historical artefact of the pre-Step-11 metadata
 * review surface and not a separate identifier space.
 */
import { apiFetch } from "./fetch";

/**
 * Accept a pending `metadata_versions` row — promotes it to canonical
 * on the manifestation/work and enqueues an OPF writeback job.
 */
export async function acceptVersion(
  manifestationId: string,
  versionId: string,
  signal?: AbortSignal,
): Promise<void> {
  await apiFetch(`/api/manifestations/${encodeURIComponent(manifestationId)}/metadata/accept`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ version_id: versionId }),
    ...(signal ? { signal } : {}),
  });
}

/**
 * Reject a pending `metadata_versions` row — marks it `status =
 * 'rejected'` and records `resolved_by`. No writeback job is enqueued
 * (the canonical pointer is unchanged).
 */
export async function rejectVersion(
  manifestationId: string,
  versionId: string,
  signal?: AbortSignal,
): Promise<void> {
  await apiFetch(`/api/manifestations/${encodeURIComponent(manifestationId)}/metadata/reject`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ version_id: versionId }),
    ...(signal ? { signal } : {}),
  });
}

/**
 * Revert a field to a specific version, or clear it entirely.
 *
 * Pass `versionId = null` to clear the canonical column (e.g. drop a
 * promoted description back to NULL). Pass a `versionId` to re-promote
 * a different pending version; the canonical pointer is rewired and a
 * writeback job is enqueued in either case.
 *
 * `title` cannot be cleared via revert (the server returns 422 — the
 * column is NOT NULL on `works`).
 */
export async function revertField(
  manifestationId: string,
  fieldName: string,
  versionId: string | null,
  signal?: AbortSignal,
): Promise<void> {
  await apiFetch(`/api/manifestations/${encodeURIComponent(manifestationId)}/metadata/revert`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ field_name: fieldName, version_id: versionId }),
    ...(signal ? { signal } : {}),
  });
}

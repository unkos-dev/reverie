/**
 * Client for the `/api/v1/shelves*` CRUD surface.
 *
 * Mirrors the response DTOs in `backend/src/models/shelf.rs`.
 *
 * # Timestamp validation
 *
 * `created_at`, `updated_at`, and `added_at` use bare `z.iso.datetime()`,
 * which accepts only a `Z`-terminated value with arbitrary sub-second
 * precision. That is exactly what the backend emits: Postgres
 * `timestamptz` normalises to UTC, and the backend serialises a UTC
 * instant with the `Z` designator. The `{ offset: true }` variant would also accept
 * `+00:00`, which no endpoint here produces, so the bare form is both
 * stricter and correct.
 *
 * These fields were `z.string()` until a serialization defect shipped a
 * non-RFC 3339 shape past them. A type-only check cannot catch a format
 * regression, so the format is pinned here rather than assumed.
 *
 * # Failure policy
 *
 * A malformed timestamp is rejected in the schema, so every consumer
 * fails at the same point rather than each inventing its own handling.
 * What differs is what the surface does with that failure. Reads fall
 * into three tiers, and the split is deliberate:
 *
 * - **Primary** surfaces let the error reach the route error boundary,
 *   because a shelves page that cannot show shelves has nothing to
 *   render: `pages/shelves/ShelvesListPage` and
 *   `pages/shelves/ShelfDetailPage` (both `useSuspenseQuery`).
 * - **Loaders** prefetch and swallow, leaving the throw to the primary
 *   surface that re-reads the same key: `routes/shelves` and
 *   `routes/shelf-detail` (both prefetch via `queryClient.query`).
 * - **Auxiliary** surfaces degrade and stay mounted, because shelves are
 *   an accessory to their real job: `components/shell/LeftRail`,
 *   `components/shell/FilterRail`, `pages/library/FilterChips`,
 *   `pages/library/BookDetailDrawer`, and `pages/library/BatchBar` (all
 *   `useQuery`). Degrading must stay observable: the central
 *   `QueryCache.onError` routes only 401s, so these log the failure
 *   themselves.
 *
 * Adding a read consumer means placing it in one of those three tiers.
 * Do not promote an auxiliary surface to a throw without a design
 * artifact covering the resulting error state.
 *
 * Writes sit outside that split and are not a fourth tier. `createShelf`
 * and `renameShelf` parse `ShelfSchema` on the response to a write the
 * server has already committed, so a rejection there reports failure for
 * an operation that succeeded: the caller's `onSuccess` never runs, its
 * cache invalidation is skipped, and retrying a create adds a second
 * shelf under the same name, because nothing enforces name uniqueness.
 * Recovery is a refetch, not a retry. Distinguishing a
 * schema rejection from a transport failure in mutation handlers is
 * error plumbing rather than contract enforcement, and is deliberately
 * not done here.
 *
 * # ETag / If-Match contract
 *
 * The backend emits `ETag: "<updated_at RFC3339>"` on every shelf read
 * and requires `If-Match` on the reorder PUT. Because the entity-tag
 * value is the timestamp itself, the frontend derives the ETag from
 * the response body's `updated_at` (`buildEtag(updatedAt)`) without
 * needing to read the response header — reads expose `updated_at`
 * directly, and mutations return 204 + ETag header but callers refetch
 * on invalidation rather than threading the header through `apiFetch`.
 *
 * The reorder mutation accepts an `ifMatch` parameter that the caller
 * carries from the cached shelf detail; the wrapper formats it as a
 * quoted entity-tag header.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

const ShelfSchema = z.object({
  id: z.string(),
  name: z.string(),
  is_system: z.boolean(),
  created_at: z.iso.datetime(),
  updated_at: z.iso.datetime(),
  item_count: z.number().int().nonnegative(),
});
/** One shelf in `GET /api/v1/shelves` (and create/rename round-trips). */
export type Shelf = z.infer<typeof ShelfSchema>;

const ShelfItemSchema = z.object({
  manifestation_id: z.string(),
  position: z.number().int(),
  added_at: z.iso.datetime(),
});
/** One item row in `GET /api/v1/shelves/{id}`. */
export type ShelfItem = z.infer<typeof ShelfItemSchema>;

/**
 * Wire page of `GET /api/v1/shelves`: the backend keyset-
 * paginates every list, so the client walks `next_cursor` internally
 * and callers keep receiving the fully assembled array.
 */
const ShelfListPageSchema = z.object({
  items: z.array(ShelfSchema),
  next_cursor: z.string().nullable(),
});

/** Wire page of `GET /api/v1/shelves/{id}` — items are keyset-paginated. */
const ShelfDetailPageSchema = z.object({
  id: z.string(),
  name: z.string(),
  is_system: z.boolean(),
  created_at: z.iso.datetime(),
  updated_at: z.iso.datetime(),
  items: z.array(ShelfItemSchema),
  next_cursor: z.string().nullable(),
});

/** Assembled shelf detail: identity plus the complete items list. */
export type ShelfWithItems = Omit<z.infer<typeof ShelfDetailPageSchema>, "next_cursor">;

/**
 * Hard bound on cursor walks. The backend terminates every walk
 * (`next_cursor` flips to null on the last page); this guard turns a
 * hypothetical server-side cursor bug into a loud error instead of an
 * infinite fetch loop.
 */
const MAX_PAGE_WALK = 100;

/** Derive an RFC 9110 quoted entity-tag from an RFC 3339 `updated_at`. */
export function buildEtag(updatedAt: string): string {
  return `"${updatedAt}"`;
}

/** List the caller's shelves (walks all pages). */
export async function listShelves(signal?: AbortSignal): Promise<Shelf[]> {
  const shelves: Shelf[] = [];
  let cursor: string | null = null;
  for (let walked = 0; walked < MAX_PAGE_WALK; walked++) {
    const path =
      cursor === null ? "/api/v1/shelves" : `/api/v1/shelves?cursor=${encodeURIComponent(cursor)}`;
    const body = await apiFetch(path, signal ? { method: "GET", signal } : { method: "GET" });
    const page = ShelfListPageSchema.parse(body);
    shelves.push(...page.items);
    cursor = page.next_cursor;
    if (cursor === null) return shelves;
  }
  throw new Error("shelves list pagination did not terminate");
}

/** Fetch a shelf and its complete items list (walks all item pages). */
export async function getShelf(id: string, signal?: AbortSignal): Promise<ShelfWithItems> {
  const base = `/api/v1/shelves/${encodeURIComponent(id)}`;
  const items: ShelfItem[] = [];
  // Identity (incl. the ETag-bearing updated_at) comes from the FIRST
  // page: under a concurrent shelf mutation mid-walk, later pages can
  // carry a newer updated_at than the items being assembled — pinning
  // page 1's identity keeps the If-Match value consistent with the
  // snapshot the caller actually received (a stale tag fails safe with
  // 412; a too-new tag could let a reorder clobber the unseen change).
  let identity: Omit<ShelfWithItems, "items"> | null = null;
  let cursor: string | null = null;
  for (let walked = 0; walked < MAX_PAGE_WALK; walked++) {
    const path = cursor === null ? base : `${base}?cursor=${encodeURIComponent(cursor)}`;
    const body = await apiFetch(path, signal ? { method: "GET", signal } : { method: "GET" });
    const page = ShelfDetailPageSchema.parse(body);
    identity ??= {
      id: page.id,
      name: page.name,
      is_system: page.is_system,
      created_at: page.created_at,
      updated_at: page.updated_at,
    };
    items.push(...page.items);
    cursor = page.next_cursor;
    if (cursor === null) {
      return { ...identity, items };
    }
  }
  throw new Error("shelf items pagination did not terminate");
}

/** Create a new shelf. Returns the freshly minted row (mirrors POST 201). */
export async function createShelf(name: string, signal?: AbortSignal): Promise<Shelf> {
  const body = await apiFetch("/api/v1/shelves", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
    ...(signal ? { signal } : {}),
  });
  return ShelfSchema.parse(body);
}

/**
 * Rename a shelf. Throws `ApiError` with `.problemSlug ===
 * "system-shelf-immutable"` (status 409) when the target shelf has
 * `is_system = true`.
 */
export async function renameShelf(id: string, name: string, signal?: AbortSignal): Promise<Shelf> {
  const body = await apiFetch(`/api/v1/shelves/${encodeURIComponent(id)}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
    ...(signal ? { signal } : {}),
  });
  return ShelfSchema.parse(body);
}

/**
 * Delete a shelf. Throws `ApiError` with `.problemSlug ===
 * "system-shelf-immutable"` (status 409) when the target is a system
 * shelf.
 */
export async function deleteShelf(id: string, signal?: AbortSignal): Promise<void> {
  await apiFetch(`/api/v1/shelves/${encodeURIComponent(id)}`, {
    method: "DELETE",
    ...(signal ? { signal } : {}),
  });
}

/** Append a manifestation to a shelf at `max(position) + 1`. */
export async function addShelfItem(
  shelfId: string,
  manifestationId: string,
  signal?: AbortSignal,
): Promise<void> {
  await apiFetch(`/api/v1/shelves/${encodeURIComponent(shelfId)}/items`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ manifestation_id: manifestationId }),
    ...(signal ? { signal } : {}),
  });
}

/** Remove a single manifestation from a shelf. */
export async function removeShelfItem(
  shelfId: string,
  manifestationId: string,
  signal?: AbortSignal,
): Promise<void> {
  await apiFetch(
    `/api/v1/shelves/${encodeURIComponent(shelfId)}/items/${encodeURIComponent(manifestationId)}`,
    {
      method: "DELETE",
      ...(signal ? { signal } : {}),
    },
  );
}

/**
 * Reorder a shelf's items. `ifMatch` MUST be the `updated_at` from
 * the cached shelf detail (the wrapper formats it as a quoted entity-
 * tag). On 412 the caller should refetch + retry with the new ETag.
 *
 * @param items - The full ordered list of manifestation ids — partial
 *   lists are rejected by the backend with 422.
 */
export async function reorderShelfItems(
  shelfId: string,
  items: string[],
  ifMatch: string,
  signal?: AbortSignal,
): Promise<void> {
  await apiFetch(`/api/v1/shelves/${encodeURIComponent(shelfId)}/items`, {
    method: "PUT",
    headers: {
      "Content-Type": "application/json",
      "If-Match": buildEtag(ifMatch),
    },
    body: JSON.stringify({ items }),
    ...(signal ? { signal } : {}),
  });
}

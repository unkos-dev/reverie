/**
 * Client for the `/api/v1/shelves*` CRUD surface.
 *
 * Mirrors the response DTOs in `backend/src/models/shelf.rs`.
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
  created_at: z.string(),
  updated_at: z.string(),
  item_count: z.number().int().nonnegative(),
});
/** One shelf in `GET /api/v1/shelves` (and create/rename round-trips). */
export type Shelf = z.infer<typeof ShelfSchema>;

const ShelfItemSchema = z.object({
  manifestation_id: z.string(),
  position: z.number().int(),
  added_at: z.string(),
});
/** One item row in `GET /api/v1/shelves/{id}`. */
export type ShelfItem = z.infer<typeof ShelfItemSchema>;

/**
 * Wire page of `GET /api/v1/shelves` (UNK-374): the backend keyset-
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
  created_at: z.string(),
  updated_at: z.string(),
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
  let cursor: string | null = null;
  for (let walked = 0; walked < MAX_PAGE_WALK; walked++) {
    const path = cursor === null ? base : `${base}?cursor=${encodeURIComponent(cursor)}`;
    const body = await apiFetch(path, signal ? { method: "GET", signal } : { method: "GET" });
    const page = ShelfDetailPageSchema.parse(body);
    items.push(...page.items);
    cursor = page.next_cursor;
    if (cursor === null) {
      return {
        id: page.id,
        name: page.name,
        is_system: page.is_system,
        created_at: page.created_at,
        updated_at: page.updated_at,
        items,
      };
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

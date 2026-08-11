/**
 * Problem-type slug for an `If-Match` optimistic-concurrency conflict.
 * Emitted with HTTP 412 by `backend/src/routes/etag.rs`'s protected PATCH
 * surfaces (reading state, metadata); the 412 response also carries the
 * resource's current `ETag` so the caller can resync without a follow-up
 * GET.
 */
export const IF_MATCH_MISMATCH_SLUG = "if-match-mismatch";

/**
 * Problem-type slug for a missing `If-Match` header on a precondition-
 * protected PATCH. Emitted with HTTP 428 by the same endpoints as
 * {@link IF_MATCH_MISMATCH_SLUG}.
 */
export const IF_MATCH_REQUIRED_SLUG = "if-match-required";

/**
 * Typed error class for failed API requests.
 *
 * Parses RFC 9457 Problem Details responses (`application/problem+json`)
 * emitted by the backend per `adr/2026-05-22-json-api-conventions.md`.
 * Every non-2xx response from `apiFetch` lands here, regardless of
 * envelope shape — if the body is not a Problem Details document the
 * `type` is `null` and `title`/`detail` fall back to `response.statusText`
 * / `""`.
 *
 * `status` is the only field downstream code should branch on; `type`
 * (the `problem-type` URI registered in `backend/src/error/problems.rs`)
 * is for user-facing copy and analytics tagging.
 */
export class ApiError extends Error {
  /** HTTP status code from the response. */
  readonly status: number;
  /** Problem-type URI from the RFC 9457 body, or `null` when the body was not Problem Details. */
  readonly type: string | null;
  /** RFC 9457 `title` field; falls back to `response.statusText` when absent. */
  readonly title: string;
  /** RFC 9457 `detail` field; empty string when absent. */
  readonly detail: string;

  /**
   * @param status - HTTP status code from the response.
   * @param type - Problem-type URI from the RFC 9457 body, or `null`
   *   when the body was not a Problem Details document.
   * @param title - RFC 9457 `title` field; falls back to
   *   `response.statusText` when absent.
   * @param detail - RFC 9457 `detail` field; empty string when absent.
   */
  /**
   * Wrap an RFC 9457 Problem Details response into a typed error.
   * Falls back to status-text / empty strings when the response body
   * was not a Problem Details document.
   */
  constructor(status: number, type: string | null, title: string, detail: string) {
    super(`${String(status)} ${title}: ${detail}`);
    this.name = "ApiError";
    this.status = status;
    this.type = type;
    this.title = title;
    this.detail = detail;
  }

  /**
   * Last path segment of the {@link type} URI — e.g.
   * `"csrf-mismatch"` for `"https://reverie.example/probs/csrf-mismatch"`.
   * Useful for switching on problem class without parsing the URI.
   *
   * @returns The slug, or `null` when `type` is null.
   */
  get problemSlug(): string | null {
    if (!this.type) return null;
    const i = this.type.lastIndexOf("/");
    return i >= 0 ? this.type.slice(i + 1) : this.type;
  }
}

/** True when `err` is a 412 `If-Match` conflict from a protected PATCH. */
export function isIfMatchMismatch(err: unknown): boolean {
  return (
    err instanceof ApiError && err.status === 412 && err.problemSlug === IF_MATCH_MISMATCH_SLUG
  );
}

/**
 * True when `err` is a 428 missing-`If-Match` rejection from a protected
 * PATCH. Reachable when an ETag-priming fetch loses the race with a very
 * fast commit; callers treat it the same as {@link isIfMatchMismatch} since
 * the remedy (reload, then retry) is identical.
 */
export function isIfMatchRequired(err: unknown): boolean {
  return (
    err instanceof ApiError && err.status === 428 && err.problemSlug === IF_MATCH_REQUIRED_SLUG
  );
}

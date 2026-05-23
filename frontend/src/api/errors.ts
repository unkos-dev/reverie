/**
 * Typed error class for failed API requests.
 *
 * Parses RFC 7807 Problem Details responses (`application/problem+json`)
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
  /** Problem-type URI from the RFC 7807 body, or `null` when the body was not Problem Details. */
  readonly type: string | null;
  /** RFC 7807 `title` field; falls back to `response.statusText` when absent. */
  readonly title: string;
  /** RFC 7807 `detail` field; empty string when absent. */
  readonly detail: string;

  /**
   * @param status - HTTP status code from the response.
   * @param type - Problem-type URI from the RFC 7807 body, or `null`
   *   when the body was not a Problem Details document.
   * @param title - RFC 7807 `title` field; falls back to
   *   `response.statusText` when absent.
   * @param detail - RFC 7807 `detail` field; empty string when absent.
   */
  /**
   * Wrap an RFC 7807 Problem Details response into a typed error.
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

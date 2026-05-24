/**
 * Centralised fetch wrapper for the `/api/*` surface.
 *
 * Owns three concerns the rest of the frontend would otherwise repeat:
 *
 * 1. **Same-origin cookies** — every request opts in. The session cookie
 *    bootstraps `CurrentUser` on the backend; without `same-origin` the
 *    SPA would be silently anonymous.
 *
 * 2. **CSRF token injection on mutating verbs** — POST/PUT/PATCH/DELETE
 *    pick up `X-CSRF-Token` from {@link getCsrfToken}; GET/HEAD/OPTIONS
 *    do not. The synchronizer-token middleware in
 *    `backend/src/security/csrf.rs` enforces presence + constant-time
 *    equality. One retry on `403 csrf-mismatch`: refresh the cache via
 *    `/auth/me`, replay once, then surface the failure.
 *
 * 3. **RFC 7807 Problem Details parsing** — non-2xx responses are
 *    funnelled into {@link ApiError} so callers branch on `.status` /
 *    `.problemSlug` instead of re-parsing JSON.
 *
 * Out of scope: route-level retries, request deduplication (react-query
 * owns that), suspense/loading state (react-query owns that too).
 */
import { ApiError } from "./errors";
import { getCsrfToken, refreshCsrfToken } from "./csrf";

const MUTATING_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);

const CSRF_MISMATCH_SLUG = "csrf-mismatch";

/** Shape of an RFC 7807 Problem Details body. All fields optional — partial responses tolerated. */
interface ProblemDetails {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  instance?: string;
}

/**
 * Issue a JSON request to a same-origin `/api/*` endpoint and parse the
 * response. The wrapper is intentionally narrow — it accepts the same
 * `init` shape as `fetch()` plus an optional override of the
 * authentication-failure handling.
 *
 * @typeParam T - Expected shape of the parsed JSON body on success.
 *   Callers pass an explicit type argument; this function does not
 *   validate the body against a schema (each `api/*` module owns its
 *   own zod schema and parses after the call).
 * @param input - URL or `Request` object, same as `fetch()`.
 * @param init - Standard `RequestInit` (with `method`, `body`, etc).
 *   `credentials` is hardcoded to `same-origin` and cannot be
 *   overridden — opting out would defeat the wrapper's purpose.
 * @returns Parsed JSON body cast to `T` on 2xx. Throws {@link ApiError}
 *   on any non-2xx; throws the underlying `TypeError` on network
 *   failure (per the WHATWG fetch spec, an aborted request rejects
 *   with `DOMException("AbortError")`).
 */
export async function apiFetch<T = unknown>(
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<T> {
  const method = (init?.method ?? "GET").toUpperCase();
  const mutating = MUTATING_METHODS.has(method);

  const response = await sendRequest(input, init, method, mutating);
  if (response.status === 403 && mutating) {
    const problem = await peekProblem(response);
    if (problem?.type && problem.type.endsWith(`/${CSRF_MISMATCH_SLUG}`)) {
      // Refresh once. If the new token is still missing we still let
      // the second attempt run — the middleware will return another
      // 403 and the caller will see a real ApiError.
      await refreshCsrfToken();
      const retried = await sendRequest(input, init, method, mutating);
      return decodeSuccess<T>(retried);
    }
    throw problemToApiError(response.status, response.statusText, problem);
  }
  return decodeSuccess<T>(response);
}

/**
 * Common success/failure decoder shared by the main path and the
 * csrf-mismatch retry path. Non-2xx throws an {@link ApiError}; 204
 * and 205 return `undefined`; everything else parses the body as
 * JSON.
 */
async function decodeSuccess<T>(response: Response): Promise<T> {
  if (!response.ok) throw await problemFromResponse(response);
  if (response.status === 204 || response.status === 205) {
    // 204 / 205 carry no body. Callers that type the return as `void`
    // or `undefined` consume this directly; callers typed as a value
    // shape would be a contract bug — flag at the call site, not here.
    return undefined as T;
  }
  // Other 2xx may also carry an empty body — e.g. the legacy
  // `/api/manifestations/{id}/metadata/{accept,reject,revert}` mutators
  // emit `200 OK` with no payload. `Response.json()` on an empty body
  // throws SyntaxError, so route through `.text()` and short-circuit
  // empties. Value-typed callers against an empty-body endpoint stay
  // a contract bug; their downstream `.parse()` will surface it.
  const text = await response.text();
  if (text.length === 0) return undefined as T;
  return JSON.parse(text) as T;
}

/**
 * Single-attempt request issuer. Builds the headers, injects the CSRF
 * token when present, and forwards to the global `fetch`. Kept separate
 * from {@link apiFetch} so the csrf-mismatch retry path can re-enter
 * with the refreshed token without recursing.
 */
async function sendRequest(
  input: RequestInfo | URL,
  init: RequestInit | undefined,
  method: string,
  mutating: boolean,
): Promise<Response> {
  const headers = new Headers(init?.headers);
  if (!headers.has("Accept")) headers.set("Accept", "application/json");
  if (mutating) {
    const token = getCsrfToken();
    if (token !== null) headers.set("X-CSRF-Token", token);
    // When `token === null` we deliberately omit the header. The
    // backend will return 428 `csrf-missing`, which surfaces as an
    // ApiError to the caller — the right place to handle "user is
    // not authenticated" rather than silently sending a blank header.
    if (!headers.has("Content-Type") && init?.body !== undefined && init.body !== null) {
      headers.set("Content-Type", "application/json");
    }
  }
  // Re-tag method explicitly so callers that omit `method` still emit
  // a normalised verb to the network layer.
  const finalInit: RequestInit = {
    ...init,
    method,
    headers,
    credentials: "same-origin",
  };
  return fetch(input, finalInit);
}

/**
 * Build an {@link ApiError} from a non-2xx response. Re-reads the body
 * as Problem Details and falls back to status-text when the body is
 * not JSON or is missing fields.
 */
async function problemFromResponse(res: Response): Promise<ApiError> {
  const problem = await peekProblem(res);
  return problemToApiError(res.status, res.statusText, problem);
}

function problemToApiError(
  status: number,
  statusText: string,
  problem: ProblemDetails | null,
): ApiError {
  return new ApiError(
    status,
    problem?.type ?? null,
    problem?.title ?? statusText,
    problem?.detail ?? "",
  );
}

/**
 * Read a response body as Problem Details. Clones the response first
 * (a `Response` body is single-shot), tolerates missing/extra fields,
 * and returns `null` when the body is not JSON-shaped.
 */
async function peekProblem(res: Response): Promise<ProblemDetails | null> {
  const ct = res.headers.get("content-type") ?? "";
  if (!ct.includes("application/problem+json") && !ct.includes("application/json")) {
    return null;
  }
  try {
    return (await res.clone().json()) as ProblemDetails;
  } catch {
    return null;
  }
}

/**
 * Centralised fetch wrapper for the `/api/v1/*` surface.
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
 * 3. **RFC 9457 Problem Details parsing** — non-2xx responses are
 *    funnelled into {@link ApiError} so callers branch on `.status` /
 *    `.problemSlug` instead of re-parsing JSON.
 *
 * Out of scope: route-level retries, request deduplication (react-query
 * owns that), suspense/loading state (react-query owns that too).
 */
import { z } from "zod";

import { ApiError } from "./errors";
import { getCsrfToken, refreshCsrfToken } from "./csrf";

const MUTATING_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);

const CSRF_MISMATCH_SLUG = "csrf-mismatch";

/**
 * Shape of an RFC 9457 Problem Details body. All fields optional —
 * partial responses tolerated. Parsed via Zod so a malformed body
 * surfaces as `null` (from `safeParse`) rather than corrupting the
 * downstream `ApiError`.
 */
const ProblemDetailsSchema = z.object({
  type: z.string().optional(),
  title: z.string().optional(),
  status: z.number().optional(),
  detail: z.string().optional(),
  instance: z.string().optional(),
});
type ProblemDetails = z.infer<typeof ProblemDetailsSchema>;

/**
 * Issue a JSON request to a same-origin `/api/v1/*` endpoint and parse the
 * response. The wrapper is intentionally narrow — it accepts the same
 * `init` shape as `fetch()` plus an optional override of the
 * authentication-failure handling.
 *
 * Returns `unknown` on 2xx. Each `api/*` module owns its Zod schema
 * and narrows the result after the call (per `frontend/CLAUDE.md` —
 * runtime validation at boundaries). Empty 2xx bodies (204/205 and
 * legacy `200 OK` mutators) resolve to `undefined`; callers that
 * ignore the return value are unaffected.
 *
 * @param input - URL or `Request` object, same as `fetch()`.
 * @param init - Standard `RequestInit` (with `method`, `body`, etc).
 *   `credentials` is hardcoded to `same-origin` and cannot be
 *   overridden — opting out would defeat the wrapper's purpose.
 * @returns Parsed JSON body as `unknown` on 2xx with body;
 *   `undefined` on 2xx with empty body. Throws {@link ApiError}
 *   on any non-2xx; throws the underlying `TypeError` on network
 *   failure (per the WHATWG fetch spec, an aborted request rejects
 *   with `DOMException("AbortError")`).
 */
export async function apiFetch(input: RequestInfo | URL, init?: RequestInit): Promise<unknown> {
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
      return decodeSuccess(retried);
    }
    throw problemToApiError(response.status, response.statusText, problem);
  }
  return decodeSuccess(response);
}

/**
 * Common success/failure decoder shared by the main path and the
 * csrf-mismatch retry path. Non-2xx throws an {@link ApiError}; 204
 * and 205 return `undefined`; everything else parses the body as
 * JSON and returns it as `unknown` for caller-side Zod narrowing.
 */
async function decodeSuccess(response: Response): Promise<unknown> {
  if (!response.ok) throw await problemFromResponse(response);
  if (response.status === 204 || response.status === 205) {
    // 204 / 205 carry no body.
    return undefined;
  }
  // Other 2xx may also carry an empty body — e.g. the legacy
  // `/api/v1/manifestations/{id}/metadata/{accept,reject,revert}` mutators
  // emit `200 OK` with no payload. `Response.json()` on an empty body
  // throws SyntaxError, so route through `.text()` and short-circuit
  // empties.
  const text = await response.text();
  if (text.length === 0) return undefined;
  try {
    return JSON.parse(text) as unknown;
  } catch (parseErr: unknown) {
    // Wrap the bare `SyntaxError` so the request URL + status + a body
    // preview reach the caller. Without this, a proxy returning HTML
    // (e.g. an upstream gateway error page) on an apparent 2xx surfaces
    // as `SyntaxError: Unexpected token <` with no context.
    const detail = parseErr instanceof Error ? parseErr.message : "JSON parse failed";
    const preview = text.length > 80 ? `${text.slice(0, 80)}…` : text;
    throw new ApiError(
      response.status,
      null,
      "Malformed JSON response",
      `${detail} (url=${response.url}, body=${preview})`,
    );
  }
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
  let raw: unknown;
  try {
    raw = await res.clone().json();
  } catch {
    return null;
  }
  const parsed = ProblemDetailsSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}

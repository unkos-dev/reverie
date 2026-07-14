import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "@/api/csrf";

// Authenticated steady state: the CSRF cache starts hydrated, so suites
// that exercise mutating API calls pin the request shape without also
// tripping apiFetch's lazy first-use hydration (a leading /auth/me fetch
// that would consume their single-response mocks). fetch.test.ts resets
// the cache itself to pin the hydration behaviour.
beforeEach(() => {
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
});

// jsdom has no ResizeObserver; cmdk (via Radix Dialog) requires it.
// No-op stub is sufficient — tests don't observe resizes themselves.
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  };
}

// jsdom implements none of the Pointer Capture API or scrollIntoView, both of
// which Radix Select touches when its trigger opens. No-op stubs let Select
// (and any Radix primitive built on pointer capture) open under test. The DOM
// lib types declare these as always present, so assign unconditionally.
Element.prototype.hasPointerCapture = (): boolean => false;
Element.prototype.setPointerCapture = (): void => {};
Element.prototype.releasePointerCapture = (): void => {};
Element.prototype.scrollIntoView = (): void => {};

// jsdom reports a zero-size layout box, so react-data-grid measures a
// zero-width viewport and renders only the first column. A fixed non-zero
// stub gives it a viewport wide enough to lay out every column under test.
Element.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 1024, 768);

afterEach(() => {
  cleanup();
});

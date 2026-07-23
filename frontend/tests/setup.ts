import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach, beforeEach } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "@/api/csrf";

// Async queries (`findBy*`, `waitFor`) default to a 1s ceiling, which the
// full suite loses under parallel load: a screen that renders comfortably in
// isolation can still be empty at 1s when setup and environment time roughly
// double against a standalone run. The ceiling only bounds how long a query
// waits before failing, so raising it costs nothing on the passing path and
// removes a class of load-dependent failure that looks like a real defect.
// 5s matches the timeout individual `waitFor` calls in the suite already
// specify by hand.
configure({ asyncUtilTimeout: 5000 });

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
// zero-size viewport and renders no rows or columns. It seeds the viewport
// from clientWidth/clientHeight (useGridDimensions) and takes other
// measurements from getBoundingClientRect; stub all three to a fixed
// non-zero box so every column and row lays out under test.
Element.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 1024, 768);
Object.defineProperty(Element.prototype, "clientWidth", { configurable: true, get: () => 1024 });
Object.defineProperty(Element.prototype, "clientHeight", { configurable: true, get: () => 768 });

afterEach(() => {
  cleanup();
});

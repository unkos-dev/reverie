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

// Formatting reads the viewer's locale and time zone. Pin them so assertions
// on formatted dates and numbers do not depend on the machine running the suite.
const VIEWER_LOCALE = "en-AU";
const VIEWER_TIME_ZONE = "UTC";

class ViewerDateTimeFormat extends Intl.DateTimeFormat {
  constructor(locales?: Intl.LocalesArgument, options?: Intl.DateTimeFormatOptions) {
    super(locales ?? VIEWER_LOCALE, { timeZone: VIEWER_TIME_ZONE, ...options });
  }
}
class ViewerNumberFormat extends Intl.NumberFormat {
  constructor(locales?: Intl.LocalesArgument, options?: Intl.NumberFormatOptions) {
    super(locales ?? VIEWER_LOCALE, options);
  }
}
class ViewerRelativeTimeFormat extends Intl.RelativeTimeFormat {
  constructor(locales?: Intl.LocalesArgument, options?: Intl.RelativeTimeFormatOptions) {
    super(locales ?? VIEWER_LOCALE, options);
  }
}
globalThis.Intl = Object.create(Intl, {
  DateTimeFormat: { value: ViewerDateTimeFormat },
  NumberFormat: { value: ViewerNumberFormat },
  RelativeTimeFormat: { value: ViewerRelativeTimeFormat },
}) as typeof Intl;
Number.prototype.toLocaleString = function toLocaleString(
  this: number,
  locales?: Intl.LocalesArgument,
  options?: Intl.NumberFormatOptions,
): string {
  return new Intl.NumberFormat(locales, options).format(this);
};

// Migration-only substitutes for browser APIs missing from jsdom.
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  };
}

// The DOM types declare these browser APIs as present, while jsdom omits them.
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

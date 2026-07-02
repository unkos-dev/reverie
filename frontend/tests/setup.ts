import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vite-plus/test";

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

afterEach(() => {
  cleanup();
});

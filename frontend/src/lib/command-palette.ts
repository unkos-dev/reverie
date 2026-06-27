/**
 * Module-level open trigger for the global command palette.
 *
 * Mirrors the `setUnauthenticatedHandler` pattern in
 * `lib/query/client.ts`: the palette component registers an opener on
 * mount, and any chrome (the utility strip's search affordance) calls
 * {@link openCommandPalette} without holding a ref into the palette's
 * React tree. Keeping the registry outside the component module avoids
 * a `react/only-export-components` violation in
 * `CommandPalette.tsx`.
 */

/**
 * Opener installed by `<CommandPalette/>` on mount. Default is a no-op
 * so a stray trigger before mount (or in tests without the palette)
 * cannot throw.
 */
let opener: () => void = () => {};

/**
 * Replace the registered opener. Called by `<CommandPalette/>` on
 * mount; pass a no-op `() => {}` on unmount to drop the stale closure.
 */
export function setCommandPaletteOpener(next: () => void): void {
  opener = next;
}

/** Open the global command palette (no-op when none is mounted). */
export function openCommandPalette(): void {
  opener();
}

/**
 * Platform-adaptive label for the palette keyboard hint (spec §6).
 * The palette's keydown handler accepts both modifiers everywhere;
 * this label only advertises the conventional one per platform.
 */
export function searchHintLabel(): string {
  // `navigator` is a browser global — guard so non-browser callers
  // (node-environment tests, future SSR) get the safe default instead
  // of a ReferenceError.
  if (typeof navigator === "undefined") return "Ctrl K";
  const nav: Navigator & { userAgentData?: { platform?: string } } = navigator;
  const platform = nav.userAgentData?.platform ?? nav.platform;
  return /mac|iphone|ipad/i.test(platform) ? "⌘K" : "Ctrl K";
}

/**
 * Platform-adaptive label for the palette keyboard hint (spec §6).
 * The focused shell accepts both modifiers;
 * this label only advertises the conventional one per platform.
 */
export function searchHintLabel(
  nav:
    | { platform: string; userAgentData?: { platform?: string } }
    | undefined = typeof navigator === "undefined" ? undefined : navigator,
): string {
  if (!nav) return "Ctrl K";
  const platform = nav.userAgentData?.platform ?? nav.platform;
  return /mac|iphone|ipad/i.test(platform) ? "⌘K" : "Ctrl K";
}

import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import { formatDateTime, formatTime, formatTimeWithAge } from "./datetime";

function viewerIs(locale: string, timeZone: string): void {
  class DateTimeFormat extends Intl.DateTimeFormat {
    constructor(locales?: Intl.LocalesArgument, options?: Intl.DateTimeFormatOptions) {
      super(locales ?? locale, { timeZone, ...options });
    }
  }
  class RelativeTimeFormat extends Intl.RelativeTimeFormat {
    constructor(locales?: Intl.LocalesArgument, options?: Intl.RelativeTimeFormatOptions) {
      super(locales ?? locale, options);
    }
  }
  vi.stubGlobal(
    "Intl",
    Object.create(Intl, {
      DateTimeFormat: { value: DateTimeFormat },
      RelativeTimeFormat: { value: RelativeTimeFormat },
    }),
  );
}

function plain(text: string): string {
  return text.replace(/[  ]/g, " ");
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("formatDateTime", () => {
  test("follows an English viewer's 12-hour clock", () => {
    viewerIs("en-AU", "UTC");
    expect(plain(formatDateTime("2026-10-09T16:05:00Z"))).toBe("9 Oct 2026, 4:05 pm");
  });

  test("follows a 24-hour viewer's order and clock", () => {
    viewerIs("de-DE", "UTC");
    expect(formatDateTime("2026-10-09T16:05:00Z")).toBe("09.10.2026, 16:05");
  });

  test("uses the viewer's time zone", () => {
    viewerIs("en-AU", "Australia/Perth");
    expect(plain(formatDateTime("2026-10-09T08:12:00Z"))).toBe("9 Oct 2026, 4:12 pm");
  });

  test("accepts a Date", () => {
    viewerIs("en-AU", "UTC");
    expect(plain(formatDateTime(new Date("2026-10-09T00:30:00Z")))).toBe("9 Oct 2026, 12:30 am");
  });
});

describe("formatTime", () => {
  test("shows only the time of day in the viewer's locale", () => {
    viewerIs("en-US", "UTC");
    expect(plain(formatTime("2026-10-09T16:05:00Z"))).toBe("4:05 PM");
    viewerIs("de-DE", "UTC");
    expect(formatTime(new Date("2026-10-09T16:05:00Z"))).toBe("16:05");
  });
});

describe("formatTimeWithAge", () => {
  const then = "2026-10-09T10:42:00Z";
  const at = (minutes: number): Date => new Date(Date.parse(then) + minutes * 60_000);

  test("reads just now inside the first minute", () => {
    viewerIs("en-AU", "UTC");
    expect(plain(formatTimeWithAge(then, at(0)))).toBe("10:42 am (just now)");
  });

  test("steps through minutes, hours and days", () => {
    viewerIs("en-AU", "UTC");
    expect(formatTimeWithAge(then, at(4))).toContain("(4 minutes ago)");
    expect(formatTimeWithAge(then, at(120))).toContain("(2 hours ago)");
    expect(formatTimeWithAge(then, at(3 * 1440))).toContain("(3 days ago)");
  });

  test("clamps a timestamp ahead of the clock to just now", () => {
    viewerIs("en-AU", "UTC");
    expect(formatTimeWithAge(then, at(-5))).toContain("(just now)");
  });

  test("phrases the age in the viewer's language", () => {
    viewerIs("fr-FR", "UTC");
    expect(plain(formatTimeWithAge(then, at(4)))).toBe("10:42 (il y a 4 minutes)");
  });
});

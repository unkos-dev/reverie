const CLOCK = new Intl.DateTimeFormat("en-AU", {
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});
const STAMP = new Intl.DateTimeFormat("en-AU", {
  day: "numeric",
  month: "short",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});
const RELATIVE = new Intl.RelativeTimeFormat("en-AU", { numeric: "always" });

/** "18:46", in the viewer's time zone. */
export function formatClock(date: Date): string {
  return CLOCK.format(date);
}

/** "9 Oct 2026, 18:31", in the viewer's time zone. */
export function formatStamp(iso: string): string {
  return STAMP.format(new Date(iso)).replace(" at ", ", ");
}

/** "18:42 (4 minutes ago)". */
export function formatClockWithAge(iso: string, now: Date): string {
  const then = new Date(iso);
  const minutes = Math.max(0, Math.round((now.getTime() - then.getTime()) / 60_000));
  return `${CLOCK.format(then)} (${ageText(minutes)})`;
}

function ageText(minutes: number): string {
  if (minutes < 1) return "just now";
  if (minutes < 60) return RELATIVE.format(-minutes, "minute");
  if (minutes < 1440) return RELATIVE.format(-Math.round(minutes / 60), "hour");
  return RELATIVE.format(-Math.round(minutes / 1440), "day");
}

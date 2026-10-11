type Instant = Date | string;

function toDate(value: Instant): Date {
  return typeof value === "string" ? new Date(value) : value;
}

/** Date and time in the viewer's locale and time zone, with the locale's own clock. */
export function formatDateTime(value: Instant): string {
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(
    toDate(value),
  );
}

/** Time of day in the viewer's locale and time zone, with the locale's own clock. */
export function formatTime(value: Instant): string {
  return new Intl.DateTimeFormat(undefined, { timeStyle: "short" }).format(toDate(value));
}

/** The time of day followed by how long ago it was, such as "6:42 pm (4 minutes ago)". */
export function formatTimeWithAge(value: Instant, now: Date): string {
  const then = toDate(value);
  const minutes = Math.max(0, Math.round((now.getTime() - then.getTime()) / 60_000));
  return `${formatTime(then)} (${ageText(minutes)})`;
}

function ageText(minutes: number): string {
  const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "always" });
  if (minutes < 1) return "just now";
  if (minutes < 60) return relative.format(-minutes, "minute");
  if (minutes < 1440) return relative.format(-Math.round(minutes / 60), "hour");
  return relative.format(-Math.round(minutes / 1440), "day");
}

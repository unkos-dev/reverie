const COUNT_FORMAT = new Intl.NumberFormat("en-AU");

/** Whole number with en-AU grouping (`1204` becomes `"1,204"`). */
function formatCount(n: number): string {
  return COUNT_FORMAT.format(n);
}

/** `"1 file"` or `"2 files"`, with the count in en-AU grouping. */
function pluralise(n: number, singular: string, plural = `${singular}s`): string {
  return `${formatCount(n)} ${n === 1 ? singular : plural}`;
}

/**
 * Day, short month and 12-hour time with a lower-case meridiem, in the
 * viewer's time zone (`"9 Oct, 4:12 pm"`). `timeZone` exists for tests.
 */
function formatWhen(iso: string, timeZone?: string): string {
  const parts = new Intl.DateTimeFormat("en-AU", {
    day: "numeric",
    month: "short",
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
    ...(timeZone === undefined ? {} : { timeZone }),
  }).formatToParts(new Date(iso));
  const get = (type: Intl.DateTimeFormatPartTypes): string =>
    parts.find((p) => p.type === type)?.value ?? "";
  const meridiem = get("dayPeriod").toLowerCase();
  return `${get("day")} ${get("month")}, ${get("hour")}:${get("minute")} ${meridiem}`;
}

/** Splits a relative path into its muted directory prefix and its file name. */
function splitPath(path: string): { dir: string; name: string } {
  const cut = path.lastIndexOf("/");
  return cut < 0
    ? { dir: "", name: path }
    : { dir: path.slice(0, cut + 1), name: path.slice(cut + 1) };
}

export { formatCount, pluralise, formatWhen, splitPath };

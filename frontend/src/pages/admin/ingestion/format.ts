const COUNT_FORMAT = new Intl.NumberFormat(undefined);

/** Whole number grouped in the viewer's locale (`1204` becomes `"1,204"` in English). */
function formatCount(n: number): string {
  return COUNT_FORMAT.format(n);
}

/** `"1 file"` or `"2 files"`, with the count grouped in the viewer's locale. */
function pluralise(n: number, singular: string, plural = `${singular}s`): string {
  return `${formatCount(n)} ${n === 1 ? singular : plural}`;
}

/** Splits a relative path into its muted directory prefix and its file name. */
function splitPath(path: string): { dir: string; name: string } {
  const cut = path.lastIndexOf("/");
  return cut < 0
    ? { dir: "", name: path }
    : { dir: path.slice(0, cut + 1), name: path.slice(cut + 1) };
}

export { formatCount, pluralise, splitPath };

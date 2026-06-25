/**
 * Read a text field from `FormData` as a string.
 *
 * `FormData.get` is typed `string | File | null`; a text input always
 * yields a string, so a non-string entry (an absent field or a file)
 * maps to the empty string rather than stringifying to `[object File]`.
 */
export function formString(data: FormData, name: string): string {
  const value = data.get(name);
  return typeof value === "string" ? value : "";
}

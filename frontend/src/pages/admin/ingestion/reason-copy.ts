import type { FailureClass } from "@/api/enrichment-failures";
import type { InputReason } from "@/api/ingestion";

type InputStatusTag = "Rejected" | "Needs a change" | "Ignored";

type InputReasonCopy = {
  /** Group heading and the name shown in another file's "Also" line. */
  label: string;
  /** The one action sentence stated once per group. */
  action: string;
  tag: InputStatusTag;
};

const INPUT_REASON_COPY: Record<InputReason, InputReasonCopy> = {
  unsafe_contents: {
    label: "Unsafe contents",
    action:
      "Reverie will not import this file. Remove it, or get the book from a source you trust.",
    tag: "Rejected",
  },
  damaged: {
    label: "The file is damaged",
    action: "Download or export the book again, then scan.",
    tag: "Rejected",
  },
  invalid_structure: {
    label: "Not a valid EPUB",
    action: "Try another copy, or repair it in an EPUB editor, then scan.",
    tag: "Rejected",
  },
  over_limits: {
    label: "Over Reverie's limits",
    action: "Remove it, or try a leaner edition of the book.",
    tag: "Rejected",
  },
  unspecified: {
    label: "Reason not specified",
    action: "Replace the file, or check the server log, then scan.",
    tag: "Rejected",
  },
  needs_change: {
    label: "Needs a change",
    action: "Check permissions and free space on the ingestion and library folders, then scan.",
    tag: "Needs a change",
  },
  retries_exhausted: {
    label: "Failed after several tries",
    action: "Check that the file opens normally, then scan to try again.",
    tag: "Needs a change",
  },
  format_not_accepted: {
    label: "Format not accepted",
    action: "Images and other non-EPUB files. They need no action. Not counted above.",
    tag: "Ignored",
  },
};

/** Group heading for a class; the ignored group carries its own prefix. */
function inputGroupTitle(reason: InputReason): string {
  return reason === "format_not_accepted"
    ? "Ignored: format not accepted"
    : INPUT_REASON_COPY[reason].label;
}

type FailureClassCopy = {
  /** Sentence-case label, used alone when the failure belongs to no source. */
  label: string;
  /** Lower-case form that follows a source name. */
  inline: string;
  /** The explanation stated once per group. */
  detail: string;
};

const FAILURE_CLASS_COPY: Record<FailureClass, FailureClassCopy> = {
  timeout: {
    label: "Timed out",
    inline: "timed out",
    detail: "The source did not answer in time. Usually temporary.",
  },
  rate_limited: {
    label: "Rate limited",
    inline: "rate limited",
    detail: "The source asked Reverie to slow down. Reverie waits and tries again.",
  },
  source_error: {
    label: "Source error",
    inline: "source error",
    detail: "The source reported a problem. Usually temporary.",
  },
  not_found: {
    label: "No record found",
    inline: "no record found",
    detail: "The source has nothing for this book. Check the book's ISBN or title.",
  },
  unreachable: {
    label: "Could not be reached",
    inline: "could not be reached",
    detail: "The request failed or the reply could not be read. Check the server's network access.",
  },
  internal: {
    label: "Internal error",
    inline: "internal error",
    detail: "Reverie hit a problem of its own. Check the server log.",
  },
  unspecified: {
    label: "Reason not specified",
    inline: "reason not specified",
    detail: "Older records land here.",
  },
};

const SOURCE_LABELS: Record<string, string> = {
  openlibrary: "Open Library",
  googlebooks: "Google Books",
  hardcover: "Hardcover",
};

/** Display name for a metadata source key; an unknown key shows as itself. */
function sourceLabel(source: string): string {
  return SOURCE_LABELS[source] ?? source;
}

type FailureRef = { source?: string | null | undefined; class: FailureClass };

/** `"Open Library: timed out"`, or the class label alone with no source. */
function failureGroupTitle(ref: FailureRef): string {
  const copy = FAILURE_CLASS_COPY[ref.class];
  return ref.source ? `${sourceLabel(ref.source)}: ${copy.inline}` : copy.label;
}

/** `"Hardcover, no record found"`, or the class label alone with no source. */
function failureAlsoLabel(ref: FailureRef): string {
  const copy = FAILURE_CLASS_COPY[ref.class];
  return ref.source ? `${sourceLabel(ref.source)}, ${copy.inline}` : copy.label;
}

export {
  INPUT_REASON_COPY,
  FAILURE_CLASS_COPY,
  inputGroupTitle,
  sourceLabel,
  failureGroupTitle,
  failureAlsoLabel,
};
export type { InputStatusTag };

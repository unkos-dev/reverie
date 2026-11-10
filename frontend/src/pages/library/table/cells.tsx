import { CircleAlert, Check, PanelRight, PenLine, Star, X } from "lucide-react";
import { useRef, useState, type ReactElement, type ReactNode, type RefObject } from "react";
import {
  Button as AriaButton,
  Dialog,
  FieldError,
  Form,
  Input,
  Link,
  ListBox,
  ListBoxItem,
  Popover,
  Tag,
  TagGroup,
  TagList,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  type KeyboardEvent,
  type PressEvent,
} from "react-aria-components";

import type { BookListItem, Density } from "@/api";
import type { ReadingStatus } from "@/api/books";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type CellState = {
  pending: string | undefined;
  failed: boolean;
};

const STATUS_OPTIONS: readonly { id: ReadingStatus; label: string }[] = [
  { id: "want_to_read", label: "Want to read" },
  { id: "reading", label: "Reading" },
  { id: "on_hold", label: "On hold" },
  { id: "finished", label: "Finished" },
  { id: "abandoned", label: "Abandoned" },
];

const STATUS_LABELS: Readonly<Record<ReadingStatus, string>> = {
  want_to_read: "Want to read",
  reading: "Reading",
  on_hold: "On hold",
  finished: "Finished",
  abandoned: "Abandoned",
};

const STARS = [1, 2, 3, 4, 5] as const;
const STAR_STEP = 18;
const STAR_INSET = 12;

const ROW_MIN: Record<Density, string> = {
  comfortable: "min-h-16",
  compact: "min-h-11",
};

const TRIGGER_MIN: Record<Density, string> = {
  comfortable: "min-h-16",
  compact: "min-h-11",
};

const WRAP = "whitespace-normal break-normal wrap-normal";

function statusLabel(status: ReadingStatus | null): string {
  return status === null ? "" : STATUS_LABELS[status];
}

function isReadingStatus(key: unknown): key is ReadingStatus {
  return typeof key === "string" && key in STATUS_LABELS;
}

type Opening = { seed: string | undefined; height: number };

function useCellEditor(anchor: RefObject<HTMLElement | null>) {
  const [opening, setOpening] = useState<Opening | null>(null);
  return {
    opening,
    open: (seed?: string): void => {
      setOpening({ seed, height: anchor.current?.offsetHeight ?? 0 });
    },
    close: (): void => {
      setOpening(null);
    },
  };
}

function editKeys(
  open: (seed?: string) => void,
  allowTyping: boolean,
  direct?: (key: string) => boolean,
): (event: KeyboardEvent) => void {
  return (event) => {
    if (event.key === "F2") {
      event.preventDefault();
      open();
      return;
    }
    if (direct?.(event.key) === true) {
      event.preventDefault();
      return;
    }
    if (
      allowTyping &&
      event.key.length === 1 &&
      event.key !== " " &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey
    ) {
      event.preventDefault();
      open(event.key);
      return;
    }
    event.continuePropagation();
  };
}

type CellPopoverProps = {
  anchor: RefObject<HTMLElement | null>;
  opening: Opening | null;
  label: string;
  overlay: boolean;
  onClose: () => void;
  className?: string;
  children: ReactNode;
};

function CellPopover({
  anchor,
  opening,
  label,
  overlay,
  onClose,
  className,
  children,
}: Readonly<CellPopoverProps>): ReactElement {
  const popoverRef = useRef<HTMLElement>(null);
  return (
    <Popover
      ref={popoverRef}
      triggerRef={anchor}
      isOpen={opening !== null}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      shouldCloseOnInteractOutside={() => {
        const form = popoverRef.current?.querySelector("form");
        if (form === null || form === undefined) return true;
        form.requestSubmit();
        return false;
      }}
      placement="bottom start"
      offset={overlay ? -(opening?.height ?? 0) : 4}
      className={cn(
        "bg-surface text-fg border-border-strong min-w-(--trigger-width) rounded-lg border shadow-md outline-none",
        className,
      )}
    >
      <Dialog aria-label={label} className="outline-none">
        {children}
      </Dialog>
    </Popover>
  );
}

function EmptyValue({ field }: Readonly<{ field: string }>): ReactElement {
  return (
    <span
      aria-hidden="true"
      className="text-fg-faint opacity-0 transition-opacity duration-150 group-data-hovered/trigger:opacity-100 group-data-focus-visible/trigger:opacity-100"
    >
      Add {field}
    </span>
  );
}

function PendingValue({ text }: Readonly<{ text: string }>): ReactElement {
  return <span className="text-fg-muted italic">{text === "unset" ? "Clearing…" : text}</span>;
}

function FailedMark(): ReactElement {
  return <CircleAlert aria-hidden="true" className="text-fg-muted ml-auto size-3.5 shrink-0" />;
}

type TriggerProps = {
  density: Density;
  field: string;
  valueText: string;
  state: CellState;
  align?: "end";
  wrap?: boolean;
  triggerRef: RefObject<HTMLButtonElement | null>;
  onOpen: (seed?: string) => void;
  allowTyping: boolean;
  direct?: (key: string) => boolean;
  onPointerPress?: (event: PressEvent) => boolean;
  onPointerMove?: (offsetX: number) => void;
  onHoverEnd?: () => void;
  renderPending?: (text: string) => ReactNode;
  children: ReactNode;
};

function Trigger({
  density,
  field,
  valueText,
  state,
  align,
  wrap = false,
  triggerRef,
  onOpen,
  allowTyping,
  direct,
  onPointerPress,
  onPointerMove,
  onHoverEnd,
  renderPending,
  children,
}: Readonly<TriggerProps>): ReactElement {
  const pending = state.pending !== undefined;
  let label = valueText === "" ? `Add ${field}` : `${valueText}, edit ${field}`;
  if (pending) label = `${field} saving`;
  if (state.failed) label += ", not saved";
  return (
    <AriaButton
      ref={triggerRef}
      aria-label={label}
      aria-haspopup="dialog"
      aria-busy={pending || undefined}
      onPress={(event) => {
        if (pending || onPointerPress?.(event) === true) return;
        onOpen();
      }}
      onKeyDown={pending ? undefined : editKeys(onOpen, allowTyping, direct)}
      onPointerMove={(event) => {
        if (event.pointerType !== "mouse" || onPointerMove === undefined) return;
        onPointerMove(event.clientX - event.currentTarget.getBoundingClientRect().left);
      }}
      onHoverChange={(hovered) => {
        if (!hovered) onHoverEnd?.();
      }}
      className={cn(
        "group/trigger relative isolate flex h-full w-full cursor-text items-center gap-2 px-3 py-3 text-left text-sm outline-none",
        "before:absolute before:inset-1 before:-z-10 before:rounded-md before:transition-colors before:duration-150 data-hovered:before:bg-surface-2 data-pressed:before:bg-surface-3",
        TRIGGER_MIN[density],
        align === "end" && "justify-end text-right",
        state.pending !== undefined && "rv-pending",
      )}
    >
      <span
        className={cn(
          "min-w-0",
          wrap ? WRAP : "truncate",
          valueText === "" && "overflow-hidden text-clip whitespace-nowrap",
          align === "end" && "tabular-nums",
        )}
      >
        {state.pending === undefined
          ? children
          : (renderPending?.(state.pending) ?? <PendingValue text={state.pending} />)}
      </span>
      {state.failed ? <FailedMark /> : null}
    </AriaButton>
  );
}

type TextKind = "text" | "pages";

type TextEditorProps = {
  field: string;
  initial: string;
  seed: string | undefined;
  kind: TextKind;
  required: boolean;
  onCommit: (value: string | null) => void;
  onCancel: () => void;
};

function validateText(
  kind: TextKind,
  required: boolean,
  field: string,
  value: string,
): string | null {
  const trimmed = value.trim();
  if (required && trimmed === "") return `A ${field} is required.`;
  if (kind === "pages" && trimmed !== "" && !/^[1-9]\d*$/.test(trimmed))
    return "Enter a whole number of pages.";
  return null;
}

function TextEditor({
  field,
  initial,
  seed,
  kind,
  required,
  onCommit,
  onCancel,
}: Readonly<TextEditorProps>): ReactElement {
  return (
    <Form
      action={(data) => {
        const raw = data.get("value");
        const trimmed = typeof raw === "string" ? raw.trim() : "";
        onCommit(trimmed === "" ? null : trimmed);
      }}
      className="flex items-start gap-1 p-1"
    >
      <TextField
        name="value"
        aria-label={field}
        defaultValue={seed ?? initial}
        validate={(value) => validateText(kind, required, field, value)}
        autoFocus
        className="min-w-0 flex-1"
      >
        <Input
          inputMode={kind === "pages" ? "numeric" : undefined}
          onFocus={(event) => {
            if (seed === undefined) event.currentTarget.select();
          }}
          className={cn(
            "border-border-control bg-canvas text-fg selection:bg-accent selection:text-fg-on-accent h-9 w-full rounded-md border px-2 text-sm outline-none",
            "data-invalid:border-danger",
            kind === "pages" && "tabular-nums",
          )}
        />
        <FieldError className="text-danger-text block px-1 pt-1.5 pb-0.5 text-xs" />
      </TextField>
      <Button type="submit" variant="ghost" size="icon" aria-label={`Save ${field}`}>
        <Check aria-hidden="true" />
      </Button>
      <Button type="button" variant="ghost" size="icon" aria-label="Cancel" onPress={onCancel}>
        <X aria-hidden="true" />
      </Button>
    </Form>
  );
}

type TextCellProps = {
  density: Density;
  field: string;
  value: string | null;
  kind: TextKind;
  required?: boolean;
  wrap?: boolean;
  align?: "end";
  mono?: boolean;
  state: CellState;
  onCommit: (value: string | null) => void;
};

export function TextCell({
  density,
  field,
  value,
  kind,
  required = false,
  wrap = false,
  align,
  mono = false,
  state,
  onCommit,
}: Readonly<TextCellProps>): ReactElement {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const editor = useCellEditor(triggerRef);
  return (
    <>
      <Trigger
        density={density}
        field={field}
        valueText={value ?? ""}
        state={state}
        align={align}
        wrap={wrap}
        triggerRef={triggerRef}
        onOpen={editor.open}
        allowTyping
      >
        {value === null ? (
          <EmptyValue field={field} />
        ) : (
          <span className={cn(mono && "font-mono text-xs")}>{value}</span>
        )}
      </Trigger>
      <CellPopover
        anchor={triggerRef}
        opening={editor.opening}
        label={`Edit ${field}`}
        overlay
        onClose={editor.close}
        className="w-(--trigger-width) min-w-72"
      >
        <TextEditor
          field={field}
          initial={value ?? ""}
          seed={editor.opening?.seed}
          kind={kind}
          required={required}
          onCommit={(next) => {
            editor.close();
            onCommit(next);
          }}
          onCancel={editor.close}
        />
      </CellPopover>
    </>
  );
}

function CoverMark({ book }: Readonly<{ book: BookListItem }>): ReactElement {
  const [failed, setFailed] = useState(false);
  if (book.cover_url !== "" && !failed) {
    return (
      <img
        src={book.cover_url}
        alt=""
        loading="lazy"
        decoding="async"
        onError={() => {
          setFailed(true);
        }}
        className="ring-border-strong h-[42px] w-[30px] shrink-0 rounded-[2px] object-cover ring-1"
      />
    );
  }
  return (
    <span
      aria-hidden="true"
      className="bg-surface-2 text-accent-text ring-border-strong font-display flex h-[42px] w-[30px] shrink-0 items-center justify-center rounded-[2px] text-[0.5rem] leading-none tracking-tight ring-1"
    >
      {book.title.slice(0, 3).toUpperCase()}
    </span>
  );
}

type TitleCellProps = {
  book: BookListItem;
  density: Density;
  showAuthors: boolean;
  state: CellState;
  onCommit: (value: string) => void;
};

export function TitleCell({
  book,
  density,
  showAuthors,
  state,
  onCommit,
}: Readonly<TitleCellProps>): ReactElement {
  const anchorRef = useRef<HTMLDivElement>(null);
  const editor = useCellEditor(anchorRef);
  const authors = book.authors.join(", ");
  return (
    <div
      ref={anchorRef}
      className={cn(
        "flex h-full items-center gap-3 px-3 py-2.5",
        ROW_MIN[density],
        state.pending !== undefined && "rv-pending",
      )}
    >
      {density === "comfortable" ? <CoverMark book={book} /> : null}
      <div className="min-w-0 flex-1">
        {state.pending === undefined ? (
          <Link
            href={`/b/${book.id}`}
            onKeyDown={editKeys(editor.open, true)}
            className={cn(
              "text-fg decoration-accent-text/60 data-hovered:underline flex min-h-6 w-fit max-w-full items-center text-sm leading-5 font-medium underline-offset-[3px] outline-none",
              WRAP,
            )}
          >
            {book.title}
          </Link>
        ) : (
          <span
            aria-busy="true"
            className={cn("flex min-h-6 items-center text-sm leading-5", WRAP)}
          >
            <PendingValue text={state.pending} />
          </span>
        )}
        {showAuthors && authors !== "" ? (
          <div className={cn("text-fg-muted text-xs leading-5", WRAP)}>{authors}</div>
        ) : null}
      </div>
      {state.failed ? <FailedMark /> : null}
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={`Edit title of ${book.title}`}
        onPress={() => {
          if (state.pending === undefined) editor.open();
        }}
        className="text-fg-muted data-hovered:text-fg -my-1 shrink-0 opacity-0 transition-opacity duration-150 group-data-hovered/row:opacity-100 group-data-focus-visible-within/row:opacity-100 data-focus-visible:opacity-100 pointer-coarse:opacity-100"
      >
        <PenLine aria-hidden="true" />
      </Button>
      <CellPopover
        anchor={anchorRef}
        opening={editor.opening}
        label="Edit title"
        overlay
        onClose={editor.close}
        className="w-(--trigger-width) min-w-80"
      >
        <TextEditor
          field="title"
          initial={book.title}
          seed={editor.opening?.seed}
          kind="text"
          required
          onCommit={(next) => {
            editor.close();
            if (next !== null) onCommit(next);
          }}
          onCancel={editor.close}
        />
      </CellPopover>
    </div>
  );
}

type AuthorDraft = { id: string; name: string };

type AuthorsEditorProps = {
  authors: readonly string[];
  seed: string | undefined;
  onCommit: (authors: string[]) => void;
  onCancel: () => void;
};

function AuthorsEditor({
  authors,
  seed,
  onCommit,
  onCancel,
}: Readonly<AuthorsEditorProps>): ReactElement {
  const [names, setNames] = useState<AuthorDraft[]>(() =>
    authors.map((name, index) => ({ id: `author-${String(index)}`, name })),
  );
  const [draft, setDraft] = useState(seed ?? "");
  const [problem, setProblem] = useState<string | null>(null);
  const nextId = useRef(authors.length);

  function add(): boolean {
    const name = draft.trim();
    if (name === "") return false;
    const id = `author-${String(nextId.current)}`;
    nextId.current += 1;
    setNames((current) => [...current, { id, name }]);
    setDraft("");
    setProblem(null);
    return true;
  }

  return (
    <Form
      action={() => {
        const pending = draft.trim();
        const final = names.map((entry) => entry.name);
        if (pending !== "") final.push(pending);
        if (final.length === 0) {
          setProblem("Add at least one author.");
          return;
        }
        onCommit(final);
      }}
      className="flex flex-col gap-3 p-3"
    >
      <TagGroup
        aria-label="Authors"
        onRemove={(keys) => {
          setNames((current) => current.filter((entry) => !keys.has(entry.id)));
        }}
      >
        <TagList
          items={names}
          renderEmptyState={() => <span className="text-fg-muted text-sm">No authors yet.</span>}
          className="flex flex-wrap gap-1.5 outline-none"
        >
          {(entry) => (
            <Tag
              id={entry.id}
              textValue={entry.name}
              className="bg-surface-2 text-fg border-border-strong data-focus-visible:outline-accent-text inline-flex items-center gap-1 rounded-full border py-0.5 pr-0.5 pl-3 text-sm outline-none data-focus-visible:outline-2 data-focus-visible:outline-offset-1"
            >
              {entry.name}
              <AriaButton
                slot="remove"
                aria-label={`Remove ${entry.name}`}
                className="text-fg-muted data-hovered:bg-surface-3 data-hovered:text-fg flex size-6 items-center justify-center rounded-full outline-none"
              >
                <X className="size-3.5" aria-hidden="true" />
              </AriaButton>
            </Tag>
          )}
        </TagList>
      </TagGroup>
      <TextField
        aria-label="Add an author"
        value={draft}
        onChange={(value) => {
          setDraft(value);
          setProblem(null);
        }}
        autoFocus
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === ",") {
            if (add() || event.key === ",") event.preventDefault();
            return;
          }
          if (event.key === "Backspace" && draft === "" && names.length > 0) {
            event.preventDefault();
            setNames((current) => current.slice(0, -1));
            return;
          }
          event.continuePropagation();
        }}
      >
        <Input
          placeholder="Add an author"
          className="border-border-control bg-canvas text-fg placeholder:text-fg-faint selection:bg-accent selection:text-fg-on-accent h-9 w-full rounded-md border px-2.5 text-sm outline-none"
        />
      </TextField>
      {problem === null ? null : (
        <p role="alert" className="text-danger-text text-xs">
          {problem}
        </p>
      )}
      <div className="flex items-center justify-between gap-3">
        <p className="text-fg-muted text-xs leading-4">Enter adds a name, then saves.</p>
        <div className="flex shrink-0 gap-1.5">
          <Button type="button" variant="ghost" size="sm" onPress={onCancel}>
            Cancel
          </Button>
          <Button type="submit" size="sm">
            Save
          </Button>
        </div>
      </div>
    </Form>
  );
}

type AuthorsCellProps = {
  book: BookListItem;
  density: Density;
  state: CellState;
  onCommit: (authors: string[]) => void;
};

export function AuthorsCell({
  book,
  density,
  state,
  onCommit,
}: Readonly<AuthorsCellProps>): ReactElement {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const editor = useCellEditor(triggerRef);
  const text = book.authors.join(", ");
  return (
    <>
      <Trigger
        density={density}
        field="authors"
        valueText={text}
        state={state}
        wrap
        triggerRef={triggerRef}
        onOpen={editor.open}
        allowTyping
      >
        {text === "" ? <EmptyValue field="authors" /> : text}
      </Trigger>
      <CellPopover
        anchor={triggerRef}
        opening={editor.opening}
        label="Edit authors"
        overlay={false}
        onClose={editor.close}
        className="w-[23rem] max-w-[calc(100vw-2rem)]"
      >
        <AuthorsEditor
          authors={book.authors}
          seed={editor.opening?.seed}
          onCommit={(authors) => {
            editor.close();
            onCommit(authors);
          }}
          onCancel={editor.close}
        />
      </CellPopover>
    </>
  );
}

type StatusCellProps = {
  density: Density;
  status: ReadingStatus | null;
  state: CellState;
  onCommit: (status: ReadingStatus | null) => void;
};

export function StatusCell({
  density,
  status,
  state,
  onCommit,
}: Readonly<StatusCellProps>): ReactElement {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const editor = useCellEditor(triggerRef);
  const label = statusLabel(status);
  return (
    <>
      <Trigger
        density={density}
        field="status"
        valueText={label}
        state={state}
        triggerRef={triggerRef}
        onOpen={editor.open}
        allowTyping={false}
        renderPending={(text) => (
          <PendingValue text={isReadingStatus(text) ? STATUS_LABELS[text] : "unset"} />
        )}
      >
        {label === "" ? (
          <EmptyValue field="status" />
        ) : (
          <span className={cn(status === "reading" ? "text-fg font-medium" : "text-fg-muted")}>
            {label}
          </span>
        )}
      </Trigger>
      <CellPopover
        anchor={triggerRef}
        opening={editor.opening}
        label="Reading status"
        overlay={false}
        onClose={editor.close}
        className="w-52 p-1"
      >
        <ListBox
          aria-label="Reading status"
          selectionMode="single"
          selectedKeys={[status ?? "none"]}
          autoFocus
          onSelectionChange={(keys) => {
            editor.close();
            if (keys === "all") return;
            const key = [...keys].at(0);
            if (key === undefined || key === (status ?? "none")) return;
            onCommit(isReadingStatus(key) ? key : null);
          }}
          className="flex flex-col gap-px outline-none"
        >
          {[...STATUS_OPTIONS, { id: "none", label: "No status" }].map((option) => (
            <ListBoxItem
              key={option.id}
              id={option.id}
              textValue={option.label}
              className={cn(
                "group/option text-fg flex cursor-default items-center gap-2 rounded-md px-2.5 py-2 text-sm outline-none data-focused:bg-surface-2",
                option.id === "none" && "border-border mt-1 border-t pt-2.5 text-fg-muted",
              )}
            >
              <Check
                aria-hidden="true"
                className="text-accent-text size-3.5 opacity-0 group-data-selected/option:opacity-100"
              />
              {option.label}
            </ListBoxItem>
          ))}
        </ListBox>
      </CellPopover>
    </>
  );
}

function starFromOffset(offsetX: number): number | null {
  const index = Math.floor((offsetX - STAR_INSET) / STAR_STEP) + 1;
  return index >= 1 && index <= 5 ? index : null;
}

function Stars({
  filled,
  muted = false,
}: Readonly<{ filled: number; muted?: boolean }>): ReactElement {
  return (
    <span className="flex items-center gap-0.5" aria-hidden="true">
      {STARS.map((star) => (
        <Star
          key={star}
          className={cn(
            "size-4 transition-[opacity,color] duration-150",
            star <= filled
              ? "text-fg"
              : "text-fg-faint opacity-0 group-data-hovered/trigger:opacity-100 group-data-focus-visible/trigger:opacity-100",
            muted && star <= filled && "text-fg-muted",
          )}
          fill={star <= filled ? "currentColor" : "none"}
          strokeWidth={1.5}
        />
      ))}
    </span>
  );
}

type RatingCellProps = {
  density: Density;
  rating: number | null;
  state: CellState;
  onCommit: (rating: number | null) => void;
};

export function RatingCell({
  density,
  rating,
  state,
  onCommit,
}: Readonly<RatingCellProps>): ReactElement {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const editor = useCellEditor(triggerRef);
  const [preview, setPreview] = useState<number | null>(null);
  const value = rating !== null && rating >= 1 ? Math.min(rating, 5) : null;
  const shown = preview ?? value ?? 0;
  const valueText = value === null ? "" : `${String(value)} of 5 stars`;

  function set(next: number | null): void {
    if (next !== value) onCommit(next);
  }

  return (
    <>
      <Trigger
        density={density}
        field="rating"
        valueText={valueText}
        state={state}
        triggerRef={triggerRef}
        onOpen={editor.open}
        allowTyping={false}
        direct={(key) => {
          if (/^[1-5]$/.test(key)) {
            set(Number(key));
            return true;
          }
          if (key === "0" || key === "Delete" || key === "Backspace") {
            set(null);
            return true;
          }
          return false;
        }}
        onPointerPress={(event) => {
          if (event.pointerType === "keyboard" || event.pointerType === "virtual") return false;
          const star = starFromOffset(event.x);
          if (star === null) return false;
          set(star === value ? null : star);
          return true;
        }}
        onPointerMove={(offsetX) => {
          setPreview(starFromOffset(offsetX));
        }}
        onHoverEnd={() => {
          setPreview(null);
        }}
        renderPending={(text) => <Stars filled={Number(text) || 0} muted />}
      >
        <Stars filled={shown} muted={preview !== null} />
      </Trigger>
      <CellPopover
        anchor={triggerRef}
        opening={editor.opening}
        label="Rating"
        overlay={false}
        onClose={editor.close}
        className="p-2"
      >
        <div className="flex items-center gap-2">
          <ToggleButtonGroup
            aria-label="Rating"
            selectionMode="single"
            selectedKeys={value === null ? [] : [String(value)]}
            onSelectionChange={(keys) => {
              editor.close();
              const key = [...keys].at(0);
              set(key === undefined ? null : Number(key));
            }}
            className="flex items-center"
          >
            {STARS.map((star) => (
              <ToggleButton
                key={star}
                id={String(star)}
                autoFocus={star === (value ?? 1)}
                aria-label={star === 1 ? "1 star" : `${String(star)} stars`}
                className={cn(
                  "text-fg-faint flex size-9 items-center justify-center rounded-md outline-none data-focus-visible:outline-2 data-focus-visible:outline-accent-text data-hovered:bg-surface-2",
                  star <= (value ?? 0) && "text-fg",
                  "[&:has(~[data-hovered])]:text-fg-muted data-hovered:text-fg-muted",
                )}
              >
                <Star
                  aria-hidden="true"
                  className="size-5"
                  strokeWidth={1.5}
                  fill={star <= (value ?? 0) ? "currentColor" : "none"}
                />
              </ToggleButton>
            ))}
          </ToggleButtonGroup>
          <Button
            variant="ghost"
            size="sm"
            isDisabled={value === null}
            onPress={() => {
              editor.close();
              set(null);
            }}
          >
            Clear
          </Button>
        </div>
      </CellPopover>
    </>
  );
}

export function DetailsButton({
  book,
  onOpen,
}: Readonly<{ book: BookListItem; onOpen: () => void }>): ReactElement {
  return (
    <div className="flex h-full items-center justify-center">
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={`Open details for ${book.title}`}
        onPress={onOpen}
        className="text-fg-muted data-hovered:text-fg opacity-0 transition-opacity duration-150 group-data-hovered/row:opacity-100 group-data-focus-visible-within/row:opacity-100 group-data-selected/row:opacity-100 data-focus-visible:opacity-100 pointer-coarse:opacity-100"
      >
        <PanelRight aria-hidden="true" />
      </Button>
    </div>
  );
}

export function StaticCell({
  density,
  align,
  children,
  className,
}: Readonly<{
  density: Density;
  align?: "end";
  children: ReactNode;
  className?: string;
}>): ReactElement {
  return (
    <div
      className={cn(
        "flex h-full items-center px-3 py-2.5 text-sm",
        ROW_MIN[density],
        align === "end" && "justify-end text-right tabular-nums",
        className,
      )}
    >
      <span className="min-w-0 truncate">{children}</span>
    </div>
  );
}

import { fireEvent, render, screen } from "@testing-library/react";
import { useState, type ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { useDebouncedCommit } from "./use-debounced-commit";

const DELAY_MS = 250;

function Harness({ onCommit }: { onCommit: (next: string) => void }): ReactElement {
  const [draft, setDraft] = useState("");
  const [committed, setCommitted] = useState("");
  const [, setTick] = useState(0);
  useDebouncedCommit(
    draft,
    committed,
    (next) => {
      onCommit(next);
      setCommitted(next);
    },
    DELAY_MS,
  );
  return (
    <div>
      <input
        aria-label="draft"
        value={draft}
        onChange={(event) => {
          setDraft(event.target.value);
        }}
      />
      <button
        type="button"
        onClick={() => {
          setTick((tick) => tick + 1);
        }}
      >
        rerender
      </button>
    </div>
  );
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useDebouncedCommit", () => {
  test("commits once, delay after the last draft change", () => {
    const onCommit = vi.fn();
    render(<Harness onCommit={onCommit} />);
    fireEvent.change(screen.getByLabelText("draft"), {
      target: { value: "ab" },
    });
    vi.advanceTimersByTime(DELAY_MS - 1);
    expect(onCommit).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith("ab");
  });

  test("unrelated parent re-renders do not reset the pending window", () => {
    const onCommit = vi.fn();
    render(<Harness onCommit={onCommit} />);
    fireEvent.change(screen.getByLabelText("draft"), {
      target: { value: "ab" },
    });
    vi.advanceTimersByTime(100);
    fireEvent.click(screen.getByRole("button", { name: "rerender" }));
    vi.advanceTimersByTime(100);
    fireEvent.click(screen.getByRole("button", { name: "rerender" }));
    vi.advanceTimersByTime(50);
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith("ab");
  });

  test("a draft change restarts the window", () => {
    const onCommit = vi.fn();
    render(<Harness onCommit={onCommit} />);
    const input = screen.getByLabelText("draft");
    fireEvent.change(input, { target: { value: "a" } });
    vi.advanceTimersByTime(150);
    fireEvent.change(input, { target: { value: "ab" } });
    vi.advanceTimersByTime(150);
    expect(onCommit).not.toHaveBeenCalled();
    vi.advanceTimersByTime(100);
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith("ab");
  });

  test("commit sees the latest callback, not the render-time closure", () => {
    const calls: string[] = [];
    function SwitchingHarness(): ReactElement {
      const [label, setLabel] = useState("first");
      const [draft, setDraft] = useState("");
      useDebouncedCommit(
        draft,
        "",
        (next) => {
          calls.push(`${label}:${next}`);
        },
        DELAY_MS,
      );
      return (
        <div>
          <input
            aria-label="draft"
            value={draft}
            onChange={(event) => {
              setDraft(event.target.value);
            }}
          />
          <button
            type="button"
            onClick={() => {
              setLabel("second");
            }}
          >
            relabel
          </button>
        </div>
      );
    }
    render(<SwitchingHarness />);
    fireEvent.change(screen.getByLabelText("draft"), {
      target: { value: "x" },
    });
    fireEvent.click(screen.getByRole("button", { name: "relabel" }));
    vi.advanceTimersByTime(DELAY_MS);
    expect(calls).toEqual(["second:x"]);
  });

  test("does not fire when the draft already equals the committed value", () => {
    const onCommit = vi.fn();
    function EqualHarness(): ReactElement {
      useDebouncedCommit("same", "same", onCommit, DELAY_MS);
      return <div />;
    }
    render(<EqualHarness />);
    vi.advanceTimersByTime(DELAY_MS * 2);
    expect(onCommit).not.toHaveBeenCalled();
  });

  test("a fire-time staleness check suppresses the pending commit", () => {
    const onCommit = vi.fn();
    let stale = false;
    function StaleHarness(): ReactElement {
      const [draft, setDraft] = useState("");
      useDebouncedCommit(draft, "", onCommit, DELAY_MS, () => stale);
      return (
        <input
          aria-label="draft"
          value={draft}
          onChange={(event) => {
            setDraft(event.target.value);
          }}
        />
      );
    }
    render(<StaleHarness />);
    fireEvent.change(screen.getByLabelText("draft"), {
      target: { value: "ab" },
    });
    // A clear affordance invalidates the draft after the timer is scheduled
    // but before it fires (no re-render in between).
    stale = true;
    vi.advanceTimersByTime(DELAY_MS * 2);
    expect(onCommit).not.toHaveBeenCalled();
  });

  test("a false staleness check leaves the commit untouched", () => {
    const onCommit = vi.fn();
    function FreshHarness(): ReactElement {
      const [draft, setDraft] = useState("");
      useDebouncedCommit(draft, "", onCommit, DELAY_MS, () => false);
      return (
        <input
          aria-label="draft"
          value={draft}
          onChange={(event) => {
            setDraft(event.target.value);
          }}
        />
      );
    }
    render(<FreshHarness />);
    fireEvent.change(screen.getByLabelText("draft"), {
      target: { value: "ab" },
    });
    vi.advanceTimersByTime(DELAY_MS);
    expect(onCommit).toHaveBeenCalledWith("ab");
  });

  test("unmount cancels a pending commit", () => {
    const onCommit = vi.fn();
    const { unmount } = render(<Harness onCommit={onCommit} />);
    fireEvent.change(screen.getByLabelText("draft"), {
      target: { value: "ab" },
    });
    vi.advanceTimersByTime(100);
    unmount();
    vi.advanceTimersByTime(DELAY_MS * 2);
    expect(onCommit).not.toHaveBeenCalled();
  });
});

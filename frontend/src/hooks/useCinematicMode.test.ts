import { act, fireEvent, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { useCinematicMode } from "./useCinematicMode";

afterEach(() => {
  document.documentElement.removeAttribute("data-cinematic");
  document.documentElement.removeAttribute("data-cursor-hidden");
});

describe("useCinematicMode", () => {
  test("F toggles cinematic mode and reflects it on the document element", () => {
    const { result } = renderHook(() => useCinematicMode());
    expect(result.current).toBe(false);

    act(() => {
      fireEvent.keyDown(window, { key: "f" });
    });
    expect(result.current).toBe(true);
    expect(document.documentElement.getAttribute("data-cinematic")).toBe("on");

    act(() => {
      fireEvent.keyDown(window, { key: "f" });
    });
    expect(result.current).toBe(false);
    expect(document.documentElement.hasAttribute("data-cinematic")).toBe(false);
  });

  test("Escape exits cinematic mode", () => {
    const { result } = renderHook(() => useCinematicMode());
    act(() => {
      fireEvent.keyDown(window, { key: "f" });
    });
    expect(result.current).toBe(true);
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    expect(result.current).toBe(false);
  });

  test("Escape is ignored while typing in a field (search stays an escape hatch)", () => {
    const { result } = renderHook(() => useCinematicMode());
    act(() => {
      fireEvent.keyDown(window, { key: "f" });
    });
    expect(result.current).toBe(true);
    const field = document.createElement("input");
    document.body.appendChild(field);
    field.focus();
    act(() => {
      fireEvent.keyDown(field, { key: "Escape" });
    });
    expect(result.current).toBe(true);
    field.remove();
  });

  test("F is ignored while typing in a field", () => {
    const { result } = renderHook(() => useCinematicMode());
    const field = document.createElement("input");
    document.body.appendChild(field);
    field.focus();
    act(() => {
      fireEvent.keyDown(field, { key: "f" });
    });
    expect(result.current).toBe(false);
    field.remove();
  });
});

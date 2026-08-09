import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import { activeUserId, forgetActiveUser, rememberActiveUser } from "./active-user";

afterEach(() => {
  localStorage.clear();
});

describe("active-user", () => {
  test("round-trips the confirmed id and forgets it on request", () => {
    expect(activeUserId()).toBeNull();
    rememberActiveUser("user-a");
    expect(activeUserId()).toBe("user-a");
    rememberActiveUser("user-b");
    expect(activeUserId()).toBe("user-b");
    forgetActiveUser();
    expect(activeUserId()).toBeNull();
  });

  test("throwing storage degrades silently in all three operations", () => {
    const getSpy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    const setSpy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    const removeSpy = vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(activeUserId()).toBeNull();
    expect(() => {
      rememberActiveUser("user-a");
    }).not.toThrow();
    expect(() => {
      forgetActiveUser();
    }).not.toThrow();
    getSpy.mockRestore();
    setSpy.mockRestore();
    removeSpy.mockRestore();
  });
});

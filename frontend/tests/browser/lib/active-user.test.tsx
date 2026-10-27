import { afterEach, describe, expect, test } from "vite-plus/test";

import { activeUserId, forgetActiveUser, rememberActiveUser } from "@/lib/active-user";

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
});

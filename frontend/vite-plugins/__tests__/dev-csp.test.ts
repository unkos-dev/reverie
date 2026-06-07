import { describe, expect, it } from "vitest";

import { buildDevCsp } from "../dev-csp";

/** Extract a single directive's value line from a built CSP string. */
function directive(csp: string, name: string): string {
  const found = csp.split("; ").find((d) => d === name || d.startsWith(`${name} `));
  if (found === undefined) throw new Error(`directive ${name} not found in: ${csp}`);
  return found;
}

/** Space-delimited tokens of a directive (drops the directive name). */
function tokens(csp: string, name: string): string[] {
  return directive(csp, name).split(" ").slice(1);
}

const TODAY_CONNECT_SRC =
  "connect-src 'self' ws://localhost:5173 ws://127.0.0.1:5173 https://cloudflareinsights.com";

// The full historical policy, pinned so any drift in a non-connect-src
// directive (a dropped origin, a changed keyword) fails loudly.
const HISTORICAL_CSP = [
  "default-src 'self'",
  "script-src 'self' 'unsafe-inline' 'unsafe-eval' https://static.cloudflareinsights.com",
  "style-src 'self' 'unsafe-inline'",
  TODAY_CONNECT_SRC,
  "img-src 'self' data:",
  "font-src 'self'",
].join("; ");

describe("buildDevCsp", () => {
  it("reproduces today's loopback connect-src exactly when REVERIE_DEV_HOSTS is unset", () => {
    expect(directive(buildDevCsp(undefined), "connect-src")).toBe(TODAY_CONNECT_SRC);
  });

  it("reproduces the full historical CSP byte-for-byte when REVERIE_DEV_HOSTS is unset", () => {
    expect(buildDevCsp(undefined)).toBe(HISTORICAL_CSP);
  });

  it("does not emit a wss origin for a .localhost subdomain", () => {
    const connect = tokens(buildDevCsp("app.localhost"), "connect-src");
    expect(connect.filter((t) => t.startsWith("wss://"))).toHaveLength(0);
    expect(connect).toContain("ws://localhost:5173");
  });

  it("propagates the upstream rejection of a directive-injecting host", () => {
    // parseAllowedHosts validates before this module sees the entry.
    expect(() => buildDevCsp("dev.example.com;default-src *")).toThrow(/REVERIE_DEV_HOSTS/);
  });

  it("reproduces today's loopback connect-src when REVERIE_DEV_HOSTS is empty", () => {
    expect(directive(buildDevCsp(""), "connect-src")).toBe(TODAY_CONNECT_SRC);
  });

  it("adds a bare wss origin for a single non-loopback host", () => {
    const connect = tokens(buildDevCsp("reverie-dev.unkos.net"), "connect-src");
    expect(connect).toContain("wss://reverie-dev.unkos.net");
    // The TLS edge terminates on 443: the origin must be the bare host, not
    // the broken `wss://<host>:5173` form the dev server would never receive.
    expect(connect).not.toContain("wss://reverie-dev.unkos.net:5173");
    expect(connect.some((t) => t.startsWith("wss://reverie-dev.unkos.net:"))).toBe(false);
  });

  it("retains the loopback ws origins alongside the derived remote origin", () => {
    const connect = tokens(buildDevCsp("reverie-dev.unkos.net"), "connect-src");
    expect(connect).toContain("ws://localhost:5173");
    expect(connect).toContain("ws://127.0.0.1:5173");
  });

  it("derives one wss origin per host in a comma-separated list", () => {
    const connect = tokens(buildDevCsp("a.example,b.example,c.example"), "connect-src");
    expect(connect).toContain("wss://a.example");
    expect(connect).toContain("wss://b.example");
    expect(connect).toContain("wss://c.example");
  });

  it("emits no stray origin from doubled or trailing commas", () => {
    const connect = tokens(buildDevCsp("a.example,,b.example,"), "connect-src");
    expect(connect).toContain("wss://a.example");
    expect(connect).toContain("wss://b.example");
    expect(connect).not.toContain("wss://");
    expect(connect.filter((t) => t.startsWith("wss://"))).toHaveLength(2);
  });

  it("does not double a loopback host supplied explicitly in the env", () => {
    const connect = tokens(buildDevCsp("localhost"), "connect-src");
    expect(connect.filter((t) => t.startsWith("wss://"))).toHaveLength(0);
    expect(connect).toContain("ws://localhost:5173");
  });

  it("keeps both Cloudflare RUM beacon origins", () => {
    const csp = buildDevCsp("reverie-dev.unkos.net");
    expect(tokens(csp, "connect-src")).toContain("https://cloudflareinsights.com");
    expect(tokens(csp, "script-src")).toContain("https://static.cloudflareinsights.com");
  });
});

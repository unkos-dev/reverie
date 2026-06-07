import { describe, expect, it } from "vitest";

import { DEFAULT_LOOPBACK_HOSTS, parseAllowedHosts } from "../allowed-hosts";

describe("parseAllowedHosts", () => {
  it("returns loopback defaults when env var is undefined", () => {
    expect(parseAllowedHosts(undefined)).toEqual(DEFAULT_LOOPBACK_HOSTS);
  });

  it("returns loopback defaults when env var is empty string", () => {
    expect(parseAllowedHosts("")).toEqual(DEFAULT_LOOPBACK_HOSTS);
  });

  it("returns loopback defaults when env var is whitespace only", () => {
    expect(parseAllowedHosts("   \t\n")).toEqual(DEFAULT_LOOPBACK_HOSTS);
  });

  it("parses a single host", () => {
    expect(parseAllowedHosts("dev.example.com")).toEqual(["dev.example.com"]);
  });

  it("parses a comma-separated list", () => {
    expect(parseAllowedHosts("a.example,b.example,c.example")).toEqual([
      "a.example",
      "b.example",
      "c.example",
    ]);
  });

  it("trims surrounding whitespace from each entry", () => {
    expect(parseAllowedHosts(" a.example ,  b.example\t,c.example ")).toEqual([
      "a.example",
      "b.example",
      "c.example",
    ]);
  });

  it("drops empty entries from doubled or trailing commas", () => {
    expect(parseAllowedHosts("a.example,,b.example,")).toEqual(["a.example", "b.example"]);
  });

  it("does not merge user list with loopback defaults", () => {
    // Replace, not merge — caller decides whether loopback access is needed.
    const result = parseAllowedHosts("dev.example.com");
    expect(result).not.toContain("localhost");
    expect(result).not.toContain("127.0.0.1");
  });

  it("rejects an entry containing whitespace", () => {
    // An embedded space would split into multiple stray CSP source tokens.
    expect(() => parseAllowedHosts("foo bar.example")).toThrow(/REVERIE_DEV_HOSTS/);
  });

  it("rejects an entry containing a semicolon (CSP directive injection)", () => {
    // A ';' would terminate the connect-src directive and inject another.
    expect(() => parseAllowedHosts("dev.example.com;default-src *")).toThrow(/REVERIE_DEV_HOSTS/);
  });

  it("rejects an entry carrying a scheme or path", () => {
    expect(() => parseAllowedHosts("https://dev.example.com")).toThrow(/REVERIE_DEV_HOSTS/);
    expect(() => parseAllowedHosts("dev.example.com/path")).toThrow(/REVERIE_DEV_HOSTS/);
  });

  it("accepts an IPv6 bracket literal", () => {
    expect(parseAllowedHosts("[::1]")).toEqual(["[::1]"]);
  });

  it("accepts a host:port entry", () => {
    expect(parseAllowedHosts("dev.example.com:8080")).toEqual(["dev.example.com:8080"]);
  });

  it("DEFAULT_LOOPBACK_HOSTS is the loopback set with bracketed IPv6", () => {
    // The IPv6 literal is bracket-enclosed so it would match Vite's
    // bracket-stripped Host comparison if Vite's hardcoded ipv6 short-circuit
    // is ever removed. See the comment in allowed-hosts.ts for details.
    expect(DEFAULT_LOOPBACK_HOSTS).toEqual(["localhost", "127.0.0.1", "[::1]"]);
  });
});

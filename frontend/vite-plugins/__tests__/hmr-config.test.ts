import { describe, expect, it } from "vite-plus/test";

import { parseHmrConfig } from "../hmr-config";

describe("parseHmrConfig", () => {
  it("returns empty object when env var is undefined", () => {
    expect(parseHmrConfig(undefined)).toEqual({});
  });

  it("returns empty object when env var is empty string", () => {
    expect(parseHmrConfig("")).toEqual({});
  });

  it("returns empty object when env var is whitespace only", () => {
    expect(parseHmrConfig("   \t\n")).toEqual({});
  });

  it("returns hmr.clientPort for a valid port", () => {
    expect(parseHmrConfig("443")).toEqual({ hmr: { clientPort: 443 } });
  });

  it("accepts the upper bound 65535", () => {
    expect(parseHmrConfig("65535")).toEqual({ hmr: { clientPort: 65535 } });
  });

  it("accepts the lower bound 1", () => {
    expect(parseHmrConfig("1")).toEqual({ hmr: { clientPort: 1 } });
  });

  it("trims surrounding whitespace from a valid port", () => {
    expect(parseHmrConfig("  443  ")).toEqual({ hmr: { clientPort: 443 } });
  });

  it("rejects 0", () => {
    expect(() => parseHmrConfig("0")).toThrow(/1\.\.=65535/);
  });

  it("rejects negative", () => {
    expect(() => parseHmrConfig("-1")).toThrow(/integer/);
  });

  it("rejects 65536", () => {
    expect(() => parseHmrConfig("65536")).toThrow(/1\.\.=65535/);
  });

  it("rejects non-numeric", () => {
    expect(() => parseHmrConfig("443abc")).toThrow(/integer/);
  });

  it("rejects floats", () => {
    expect(() => parseHmrConfig("443.5")).toThrow(/integer/);
  });

  it("rejects scientific notation", () => {
    expect(() => parseHmrConfig("1e3")).toThrow(/integer/);
  });

  it("rejects hex notation", () => {
    expect(() => parseHmrConfig("0x1bb")).toThrow(/integer/);
  });

  it("rejects signed values", () => {
    expect(() => parseHmrConfig("+443")).toThrow(/integer/);
  });
});

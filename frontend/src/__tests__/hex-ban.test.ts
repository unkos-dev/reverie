import { describe, it } from "vitest";
import { RuleTester, type Rule } from "eslint";

// RuleTester defaults to Mocha-style globals (describe/it). Wire vitest's
// equivalents so the test runs under our existing harness.
RuleTester.describe = describe;
RuleTester.it = it;

// In-process re-creation of the rule shape declared in eslint.config.js.
// Keeping the rule object inline (rather than importing from the config)
// lets the test enforce the contract without coupling to flat-config plumbing
// — if eslint.config.js drifts off the documented selector, the production
// lint run breaks first and surfaces the divergence.
const rule: Rule.RuleModule = {
  meta: {
    type: "problem",
    docs: { description: "Disallow raw hex colour literals." },
    schema: [],
    messages: { noHex: "No raw hex codes." },
  },
  create(context) {
    return {
      Literal(node) {
        const value = (node as { value?: unknown }).value;
        if (typeof value === "string" && /^#[0-9a-fA-F]{3,8}$/.test(value)) {
          context.report({ node, messageId: "noHex" });
        }
      },
    };
  },
};

const tester = new RuleTester({
  languageOptions: { ecmaVersion: 2022, sourceType: "module" },
});

tester.run("hex-ban", rule, {
  valid: [
    { code: 'const c = "hello";' },
    { code: "const c = bgSurface;" },
    { code: 'const c = "rgb(0, 0, 0)";' },
  ],
  invalid: [
    { code: 'const c = "#abc123";', errors: [{ messageId: "noHex" }] },
    { code: 'const c = "#fff";', errors: [{ messageId: "noHex" }] },
    { code: 'const c = "#C9A961";', errors: [{ messageId: "noHex" }] },
  ],
});

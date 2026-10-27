import "@/index.css";
import "vitest-browser-react";

import { beforeEach } from "vite-plus/test";
import { __seedCsrfTokenForTesting } from "@/api/csrf";

beforeEach(() => {
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
});

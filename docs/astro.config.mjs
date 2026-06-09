// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightOpenAPI, { openAPISidebarGroups } from "starlight-openapi";

export default defineConfig({
  site: "https://unkos-dev.github.io",
  base: "/reverie",
  integrations: [
    starlight({
      title: "Reverie",
      plugins: [
        starlightOpenAPI([
          {
            base: "api",
            schema: "./openapi.json",
            label: "API Reference",
          },
        ]),
      ],
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/unkos-dev/reverie",
        },
      ],
      sidebar: [
        {
          label: "Getting Started",
          items: [{ label: "Introduction", slug: "getting-started/introduction" }],
        },
        {
          label: "Design",
          items: [{ autogenerate: { directory: "design" } }],
        },
        {
          label: "Reference",
          items: [{ autogenerate: { directory: "reference" } }],
        },
        ...openAPISidebarGroups,
      ],
    }),
  ],
});

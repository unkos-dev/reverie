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
      logo: { src: "./src/assets/slot.svg", alt: "" },
      customCss: ["./src/styles/custom.css"],
      head: [
        {
          tag: "link",
          attrs: {
            rel: "icon",
            type: "image/png",
            sizes: "32x32",
            href: "/reverie/brand/favicon-32.png",
          },
        },
        {
          tag: "link",
          attrs: {
            rel: "icon",
            type: "image/png",
            sizes: "16x16",
            href: "/reverie/brand/favicon-16.png",
          },
        },
        {
          tag: "link",
          attrs: {
            rel: "apple-touch-icon",
            sizes: "180x180",
            href: "/reverie/brand/apple-touch-icon-180.png",
          },
        },
        {
          tag: "meta",
          attrs: {
            property: "og:image",
            content: "https://unkos-dev.github.io/reverie/brand/og-card-1200x630.png",
          },
        },
        { tag: "meta", attrs: { property: "og:image:width", content: "1200" } },
        { tag: "meta", attrs: { property: "og:image:height", content: "630" } },
      ],
      plugins: [
        starlightOpenAPI([
          {
            base: "api",
            schema: "../backend/openapi.json",
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
          items: [
            { label: "Introduction", slug: "getting-started/introduction" },
            { label: "Navigating Reverie", slug: "getting-started/navigation" },
          ],
        },
        {
          label: "Guides",
          items: [{ autogenerate: { directory: "guides" } }],
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

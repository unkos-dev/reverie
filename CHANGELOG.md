# Changelog

## 0.1.0 (2026-04-28)


### Features

* **api:** add application skeleton with config, DB pool, error handling, and health endpoints ([#8](https://github.com/unkos-dev/reverie/issues/8)) ([2254846](https://github.com/unkos-dev/reverie/commit/2254846e7c4585b5f4575ebf2531d020acb04b95))
* **api:** minimal Axum server with health endpoint ([ea81ec8](https://github.com/unkos-dev/reverie/commit/ea81ec8148d2cee4679139f69861b71de4670b99))
* **auth:** add OIDC authentication and device token support ([#10](https://github.com/unkos-dev/reverie/issues/10)) ([17c65a3](https://github.com/unkos-dev/reverie/commit/17c65a3045b09a87c313cd9bc29960e9b3d1b055))
* **db:** add PostgreSQL schema with 4-role architecture and RLS ([#6](https://github.com/unkos-dev/reverie/issues/6)) ([7d07a04](https://github.com/unkos-dev/reverie/commit/7d07a04d9eae956e6e2859807e8ed84d5195595c))
* **design-system:** D3 — canonical theme, fonts, FOUC, gallery (UNK-103) ([#52](https://github.com/unkos-dev/reverie/issues/52)) ([acbda61](https://github.com/unkos-dev/reverie/commit/acbda61d4b8cbc707a97401319a6e375a4384fc4))
* **design-system:** Step 10 D0–D2 + brand identity install ([#51](https://github.com/unkos-dev/reverie/issues/51)) ([4febd8e](https://github.com/unkos-dev/reverie/commit/4febd8e9354bb7d6df70a846b9810f54dbbf51ea))
* **enrichment:** metadata enrichment pipeline (blueprint step 7) ([#15](https://github.com/unkos-dev/reverie/issues/15)) ([2e5a35d](https://github.com/unkos-dev/reverie/commit/2e5a35d5c863ceedde780eeea33aab5e1069b238))
* **epub:** add EPUB structural validation and auto-repair pipeline ([#13](https://github.com/unkos-dev/reverie/issues/13)) ([c939c4f](https://github.com/unkos-dev/reverie/commit/c939c4fa26c439b2ea8556e3369fe846664878ef))
* **ingestion:** add file ingestion pipeline ([#11](https://github.com/unkos-dev/reverie/issues/11)) ([f5c5c39](https://github.com/unkos-dev/reverie/commit/f5c5c39a141d27092ba24e504f6de818f928e4cc))
* **metadata:** OPF metadata extraction and schema hardening ([#14](https://github.com/unkos-dev/reverie/issues/14)) ([c47444b](https://github.com/unkos-dev/reverie/commit/c47444bdbb76d6fefb5fbb27a771765b743c668d))
* **opds:** OPDS 1.2 catalog (BLUEPRINT Step 9) ([#26](https://github.com/unkos-dev/reverie/issues/26)) ([44e0223](https://github.com/unkos-dev/reverie/commit/44e02239e82b7bb34d07b667dc3ea71507d7395e))
* **security:** add CSP and bundled security headers (UNK-106) ([#50](https://github.com/unkos-dev/reverie/issues/50)) ([f070b97](https://github.com/unkos-dev/reverie/commit/f070b977e4cd3c04f1c2f02c1c9856366f615ebb))
* **ui:** scaffold React + Vite + TypeScript + Tailwind frontend ([c874fca](https://github.com/unkos-dev/reverie/commit/c874fcaee0f6271b5dd629f1c733cc6dd4b397b2))
* **writeback:** metadata writeback pipeline (blueprint step 8) ([#19](https://github.com/unkos-dev/reverie/issues/19)) ([d65fb4d](https://github.com/unkos-dev/reverie/commit/d65fb4d75d33bf124e1573411d82db315e463cb7))


### Bug Fixes

* **api:** update rand 0.10.0 → 0.10.1 (GHSA-cq8v-f236-94qc) ([#9](https://github.com/unkos-dev/reverie/issues/9)) ([1881f57](https://github.com/unkos-dev/reverie/commit/1881f57fa9152c8c42dca8317fd6df5f0b9f55e5))
* **auth:** replace stringly-typed user role with typed Role enum (UNK-108) ([#54](https://github.com/unkos-dev/reverie/issues/54)) ([cd4f5e7](https://github.com/unkos-dev/reverie/commit/cd4f5e7ead8fc2043d319da0cd9a73bcaa871ad7))
* **ci:** add minimum permissions to workflow ([#12](https://github.com/unkos-dev/reverie/issues/12)) ([7c6dac1](https://github.com/unkos-dev/reverie/commit/7c6dac1612ca830f304b294342658e9e664ecdf5))
* **ci:** add version.txt for release-please ([#2](https://github.com/unkos-dev/reverie/issues/2)) ([8ae19f2](https://github.com/unkos-dev/reverie/commit/8ae19f2740384406d631fe64432010ac7b0edc5e))
* **ci:** always run CodeQL actions analyzer to satisfy ruleset code-scanning gate ([#48](https://github.com/unkos-dev/reverie/issues/48)) ([556a619](https://github.com/unkos-dev/reverie/commit/556a619ae93858af5427b7ffe7292e7bf24fac9a))
* **ci:** set initial-version to 0.1.0 for release-please ([#4](https://github.com/unkos-dev/reverie/issues/4)) ([95f30ff](https://github.com/unkos-dev/reverie/commit/95f30ff15597c50beb678609bd7aca00f2fce6c7))
* **deps:** bump rustls-webpki 0.103.12 → 0.103.13 (RUSTSEC-2026-0104) ([#31](https://github.com/unkos-dev/reverie/issues/31)) ([50bbad9](https://github.com/unkos-dev/reverie/commit/50bbad99233ed9e505ae4ca436af321421df5cdd))
* **design-system:** D3 spec corrections (UNK-114) ([#57](https://github.com/unkos-dev/reverie/issues/57)) ([0754aa8](https://github.com/unkos-dev/reverie/commit/0754aa84b51cce67088aabfb59f69d8a1681dd87))
* **docs:** bump astro 6.1.5 → 6.1.8 to patch XSS in define:vars ([#30](https://github.com/unkos-dev/reverie/issues/30)) ([19b5f5f](https://github.com/unkos-dev/reverie/commit/19b5f5fce4422127f40732d3081591ab0fdbd8a4))
* **enrichment:** UNK-96 follow-ups (silent failures + phase tests) ([#60](https://github.com/unkos-dev/reverie/issues/60)) ([2c7d8a1](https://github.com/unkos-dev/reverie/commit/2c7d8a144d4a482dd4dfb1e413a8c7fc4cb8c3cc))
* lowercase repo slug in URLs after GitHub rename ([#17](https://github.com/unkos-dev/reverie/issues/17)) ([ce2b3c6](https://github.com/unkos-dev/reverie/commit/ce2b3c660d7c09e29704eaa896a32c00c746a449))
* **rls:** gate manifestations system policies on explicit GUC ([#24](https://github.com/unkos-dev/reverie/issues/24)) ([89fb486](https://github.com/unkos-dev/reverie/commit/89fb486ce8fb3d3b67e61605ae71f044d5923809))

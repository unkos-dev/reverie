# Changelog

## 0.1.0 (2026-10-17)


### ⚠ BREAKING CHANGES

* **auth:** The API no longer accepts identity-provider access tokens, and the `REVERIE_RESOURCE_SERVER_*` settings are removed. API clients must use Reverie-issued scoped tokens instead. OIDC browser sign-in remains supported.

### Features

* **config:** accept credentials from mounted files ([#1360](https://github.com/unkos-dev/reverie/issues/1360)) ([49b2a9d](https://github.com/unkos-dev/reverie/commit/49b2a9dedd74f86366ae971608f7c691af9258bd))
* **enrichment:** list books whose enrichment is failing under bounded classes ([#1378](https://github.com/unkos-dev/reverie/issues/1378)) ([11b0d81](https://github.com/unkos-dev/reverie/commit/11b0d81e2224379e77e983d780cc17e5e92a0ef4))
* **frontend:** add the admin settings screen ([#1377](https://github.com/unkos-dev/reverie/issues/1377)) ([a81374d](https://github.com/unkos-dev/reverie/commit/a81374d870267f11f47ce29b526d450a18b1c0c7))
* **ingestion:** expose admission control and runtime blockers ([#1459](https://github.com/unkos-dev/reverie/issues/1459)) ([132c3eb](https://github.com/unkos-dev/reverie/commit/132c3eb051179398e0eeee46d5cf38b689fcb11a))
* **ingestion:** list inputs needing attention under bounded reason classes ([#1376](https://github.com/unkos-dev/reverie/issues/1376)) ([9143da3](https://github.com/unkos-dev/reverie/commit/9143da3a152006582069b2d36b836d593bb3ae33))
* **ingestion:** make import suspension visible ([#1461](https://github.com/unkos-dev/reverie/issues/1461)) ([85fff85](https://github.com/unkos-dev/reverie/commit/85fff85f59ebb7348d50f03627fa615fbff0b982))
* **ingestion:** retry failure groups without bypassing scheduling ([#1464](https://github.com/unkos-dev/reverie/issues/1464)) ([d9767e7](https://github.com/unkos-dev/reverie/commit/d9767e739db67bbedac7aa763b001b98669b6eda))
* **runtime:** recover failed background workers ([#1452](https://github.com/unkos-dev/reverie/issues/1452)) ([d06786e](https://github.com/unkos-dev/reverie/commit/d06786e9c9505edb19130caea1050c1ba3d18417))
* **settings:** require If-Match on PUT /api/v1/settings ([#1369](https://github.com/unkos-dev/reverie/issues/1369)) ([4aeb4b8](https://github.com/unkos-dev/reverie/commit/4aeb4b83f0331c96efbb47fea2967e4860bb9d30))
* **ui:** add ingestion scan control, grouped failure lists and empty-library guidance ([#1391](https://github.com/unkos-dev/reverie/issues/1391)) ([5114156](https://github.com/unkos-dev/reverie/commit/511415697e4f8e3c11409de760005e6227f13421))
* **ui:** reset the default Tailwind colour ramps and guard their absence ([#1010](https://github.com/unkos-dev/reverie/issues/1010)) ([0a72b7b](https://github.com/unkos-dev/reverie/commit/0a72b7b4a2bc8a8c61717434709e51dfc6285a44))


### Bug Fixes

* **api:** bound the identifier and provider maps and declare the bounds ([#1014](https://github.com/unkos-dev/reverie/issues/1014)) ([b681942](https://github.com/unkos-dev/reverie/commit/b681942062fe0901fda8772c7e027d19f5a9440a))
* **api:** emit problem details on 405 responses ([#1005](https://github.com/unkos-dev/reverie/issues/1005)) ([aad6840](https://github.com/unkos-dev/reverie/commit/aad68409eb1be292cb2ebb43aa7f7bd086406550))
* **api:** keep Problem Details media types accurate ([#1048](https://github.com/unkos-dev/reverie/issues/1048)) ([e51cb0b](https://github.com/unkos-dev/reverie/commit/e51cb0b762fdd6a4f4969b5fb138874045e7003e))
* **api:** leave pending metadata proposals out of book detail for a child caller ([#1202](https://github.com/unkos-dev/reverie/issues/1202)) ([9531704](https://github.com/unkos-dev/reverie/commit/9531704d46981e7e7beaa5237750995972bbe104))
* **api:** make generated OpenAPI operation ids unique and schema types explicit ([#1006](https://github.com/unkos-dev/reverie/issues/1006)) ([44678ab](https://github.com/unkos-dev/reverie/commit/44678ab8da2af57bcb7d4da301d426f4202c872b))
* **api:** return Problem Details for malformed path parameters and JSON bodies ([#1052](https://github.com/unkos-dev/reverie/issues/1052)) ([4524846](https://github.com/unkos-dev/reverie/commit/45248467c97598725da02191f94edddda5d9ac56))
* **api:** serve the dist files that live outside assets ([#872](https://github.com/unkos-dev/reverie/issues/872)) ([d25b20a](https://github.com/unkos-dev/reverie/commit/d25b20a641e22d1229c6e3f6b50b5d5bc899371b))
* **auth:** prevent stale changes and inconsistent password policy ([#1328](https://github.com/unkos-dev/reverie/issues/1328)) ([ed5ae89](https://github.com/unkos-dev/reverie/commit/ed5ae8996c146f663f32c05c84f9c9097ef5c27e))
* **auth:** refuse a disabled account before an OIDC login writes to it ([#1371](https://github.com/unkos-dev/reverie/issues/1371)) ([be641f4](https://github.com/unkos-dev/reverie/commit/be641f490651ec8635e53cf3beb5683ee649cc65))
* **auth:** refuse an OIDC sign-in whose email belongs to another account ([#1389](https://github.com/unkos-dev/reverie/issues/1389)) ([144d156](https://github.com/unkos-dev/reverie/commit/144d156423942f2ed0b0e768138bffd1ff52a27c))
* **ci:** reject incomplete SQL linting ([#900](https://github.com/unkos-dev/reverie/issues/900)) ([1576add](https://github.com/unkos-dev/reverie/commit/1576add5bce7ded5dec73b108590fe128dfbb4a0))
* **ci:** run Rust build scripts during CodeQL extraction ([#1364](https://github.com/unkos-dev/reverie/issues/1364)) ([b211427](https://github.com/unkos-dev/reverie/commit/b211427643afa82682ac733de3af88a461a17c20))
* **codeql:** provide required Rust results on pull requests ([#1065](https://github.com/unkos-dev/reverie/issues/1065)) ([e679903](https://github.com/unkos-dev/reverie/commit/e6799031f4171ac456ad36a2fd3c48a7217dab88))
* **config:** keep credentials out of diagnostic output ([#1344](https://github.com/unkos-dev/reverie/issues/1344)) ([cd9af72](https://github.com/unkos-dev/reverie/commit/cd9af72c31910c85a59019811fd906dd72a28c83))
* **config:** reject an operator contact that cannot be a header value ([#1039](https://github.com/unkos-dev/reverie/issues/1039)) ([d7d4725](https://github.com/unkos-dev/reverie/commit/d7d47256d94306f77a76f11133265a3820fd54af))
* **covers:** bind cached covers to their owning library ([#1311](https://github.com/unkos-dev/reverie/issues/1311)) ([d2ec859](https://github.com/unkos-dev/reverie/commit/d2ec859fae09b14007abf6af73ea166c08cfb382))
* **db:** isolate verification and remove credential defaults ([#1353](https://github.com/unkos-dev/reverie/issues/1353)) ([4c942cc](https://github.com/unkos-dev/reverie/commit/4c942ccd35145bce5d750df7297b3ad62b570921))
* **db:** scope taxonomy and tag junction rows through manifestation visibility ([#1007](https://github.com/unkos-dev/reverie/issues/1007)) ([14b8da3](https://github.com/unkos-dev/reverie/commit/14b8da390bdbbe1ff6e6e111af112f8c57451931))
* **db:** stop the reporting role from reading session IDs ([#1084](https://github.com/unkos-dev/reverie/issues/1084)) ([46573a1](https://github.com/unkos-dev/reverie/commit/46573a1fd3bdee4f4618f04fec53d86e047f3359))
* **deps:** align utoipa and its axum integration on v6 ([#1254](https://github.com/unkos-dev/reverie/issues/1254)) ([50767da](https://github.com/unkos-dev/reverie/commit/50767daff5f87d8861c885b3b313766331208c81))
* **deps:** avoid manual merges for routine mise executable updates ([#1270](https://github.com/unkos-dev/reverie/issues/1270)) ([01dbfef](https://github.com/unkos-dev/reverie/commit/01dbfef328448b431a35ecb20996f74934f86618))
* **deps:** avoid manual merges for routine mise updates ([#1269](https://github.com/unkos-dev/reverie/issues/1269)) ([6c7a4b7](https://github.com/unkos-dev/reverie/commit/6c7a4b781cb91036c2ceac523d2dcd6af4d70178))
* **deps:** update astro-docs ([#1004](https://github.com/unkos-dev/reverie/issues/1004)) ([09d3aa8](https://github.com/unkos-dev/reverie/commit/09d3aa8b27772896fd2cda853d20d8aa34b572f3))
* **deps:** update dependency nuqs to v2.10.1 ([#977](https://github.com/unkos-dev/reverie/issues/977)) ([54de945](https://github.com/unkos-dev/reverie/commit/54de9458d579e5979f8b5d1175256b695ed8ec84))
* **deps:** update dependency nuqs to v2.10.2 ([#1422](https://github.com/unkos-dev/reverie/issues/1422)) ([d5f744a](https://github.com/unkos-dev/reverie/commit/d5f744af31680872cb8799c051ea66842be3390a))
* **deps:** update dependency starlight-openapi to ^0.27.0 ([#1401](https://github.com/unkos-dev/reverie/issues/1401)) ([392803a](https://github.com/unkos-dev/reverie/commit/392803a98214ac8948a828ffb99ad9bc5f6bb77a))
* **deps:** update rust crate argon2 to 0.6.0 ([#988](https://github.com/unkos-dev/reverie/issues/988)) ([8df4e06](https://github.com/unkos-dev/reverie/commit/8df4e06fd0687678f787257fabe1b56c47d6c4af))
* **deps:** update rust crate quick-xml to 0.42.0 ([#964](https://github.com/unkos-dev/reverie/issues/964)) ([d386fb1](https://github.com/unkos-dev/reverie/commit/d386fb12feff0f89d4c647def3d75aa6044c2eb3))
* **deps:** update rust crate zip to v9 ([#1463](https://github.com/unkos-dev/reverie/issues/1463)) ([a996473](https://github.com/unkos-dev/reverie/commit/a9964735cb11ddd608272188f2f13e70194e95bd))
* **enrichment:** keep Google Books keys out of source errors ([#1345](https://github.com/unkos-dev/reverie/issues/1345)) ([da964a6](https://github.com/unkos-dev/reverie/commit/da964a6103b62d0d651203793df1693e46ae3ea8))
* **enrichment:** reclaim orphaned jobs on startup ([#1212](https://github.com/unkos-dev/reverie/issues/1212)) ([63734ed](https://github.com/unkos-dev/reverie/commit/63734ed665ff068ec1e563bf7b685fbc0fd13f8e))
* **enrichment:** record a rate-limit deferral apart from the attempt time ([#1409](https://github.com/unkos-dev/reverie/issues/1409)) ([7871d6d](https://github.com/unkos-dev/reverie/commit/7871d6dbde31f22cf5065fd8f462e00cc97c80d5))
* **enrichment:** recover claims when a run panics ([#1260](https://github.com/unkos-dev/reverie/issues/1260)) ([f4bbe21](https://github.com/unkos-dev/reverie/commit/f4bbe21ea45e87fb1e5a3a08f232ecb89f06413c))
* **enrichment:** validate every redirect hop the metadata client follows ([#1198](https://github.com/unkos-dev/reverie/issues/1198)) ([c3c1d14](https://github.com/unkos-dev/reverie/commit/c3c1d14bf66ba456e1314ddfe621a0323714ecd5))
* **epub:** read archives with rawzip so the entry cap runs before any header is parsed ([#1102](https://github.com/unkos-dev/reverie/issues/1102)) ([77ccf7e](https://github.com/unkos-dev/reverie/commit/77ccf7e5f7d96a23c5eee23ef61787abf23694b7))
* **epub:** reject undecodable covers and bound path collisions ([#1309](https://github.com/unkos-dev/reverie/issues/1309)) ([fe8e55f](https://github.com/unkos-dev/reverie/commit/fe8e55ff2ce62d2cc04c44c2fa397b06bd425d9b))
* **epub:** repair a mimetype entry that breaks the OCF container rules ([#1118](https://github.com/unkos-dev/reverie/issues/1118)) ([88d980a](https://github.com/unkos-dev/reverie/commit/88d980a1e8cd942ddb7ddc261b8a248f399e5276))
* **files:** preserve sources after failed relocation and mixed ingestion ([#1272](https://github.com/unkos-dev/reverie/issues/1272)) ([c8b3b55](https://github.com/unkos-dev/reverie/commit/c8b3b55e14ad42247d844fedbf61f1b5d206f9e6))
* **frontend:** comply with locale-aware sorting rule ([#1431](https://github.com/unkos-dev/reverie/issues/1431)) ([386b7b3](https://github.com/unkos-dev/reverie/commit/386b7b35eb3fc280470331ee31e8bfd6572aa3cc))
* **ingestion:** refuse startup without dedicated credentials ([#1327](https://github.com/unkos-dev/reverie/issues/1327)) ([65bdbad](https://github.com/unkos-dev/reverie/commit/65bdbadad4695bee7327d1dd26e080928a9c2f6f))
* **ingestion:** reject corrupt EPUBs instead of importing them ([#1398](https://github.com/unkos-dev/reverie/issues/1398)) ([fa6f895](https://github.com/unkos-dev/reverie/commit/fa6f895bac36bc8733b4f6ec2ac2cd417156e668))
* **ingestion:** release test ownership before the next scan ([#1444](https://github.com/unkos-dev/reverie/issues/1444)) ([79effc8](https://github.com/unkos-dev/reverie/commit/79effc857202635f0fcdf15bdda4a722b441cb51))
* **ingestion:** retain inputs across failures and retries ([#1302](https://github.com/unkos-dev/reverie/issues/1302)) ([1b3f920](https://github.com/unkos-dev/reverie/commit/1b3f920c2a22a059376a159fe566672b119b1608))
* **just:** keep stale pid advice executable ([#1139](https://github.com/unkos-dev/reverie/issues/1139)) ([8ef6e41](https://github.com/unkos-dev/reverie/commit/8ef6e41c7a7b41e05fa9b20382fa96b157dac59a))
* **library:** answer 422 for sort violations after decode ([#1372](https://github.com/unkos-dev/reverie/issues/1372)) ([88d006c](https://github.com/unkos-dev/reverie/commit/88d006c92795b04d4268845f8936e55d98d4149f))
* **library:** prevent inverted filter ranges ([#1210](https://github.com/unkos-dev/reverie/issues/1210)) ([99084f5](https://github.com/unkos-dev/reverie/commit/99084f5d684fdb83f9ed72a2cb447f827159e691))
* **library:** separate filter clearing from disclosure controls ([#1276](https://github.com/unkos-dev/reverie/issues/1276)) ([cc8c144](https://github.com/unkos-dev/reverie/commit/cc8c1446cea81c672ff3b01435f362f5f13eabe3))
* **metadata:** preserve edits committed during enrichment ([#1274](https://github.com/unkos-dev/reverie/issues/1274)) ([71a1a9c](https://github.com/unkos-dev/reverie/commit/71a1a9cb312eb4c916e2bc0216321b8ae6e66d3d))
* **metadata:** reject non-pending versions during accept ([#1213](https://github.com/unkos-dev/reverie/issues/1213)) ([15b0934](https://github.com/unkos-dev/reverie/commit/15b093450ed69ebb3464f2f33a837f40da21125a))
* **metadata:** return 404 for missing field-lock manifestations ([#1211](https://github.com/unkos-dev/reverie/issues/1211)) ([2cea1ae](https://github.com/unkos-dev/reverie/commit/2cea1ae58ec8d29fbf4071a90b7fa135d1abb1a7))
* **openapi:** declare the extractor failure statuses on every operation ([#1388](https://github.com/unkos-dev/reverie/issues/1388)) ([19b2334](https://github.com/unkos-dev/reverie/commit/19b2334b81dadb1e6dcb917a071f5ba0cf9c70e0))
* **openapi:** publish existing request bounds ([#1445](https://github.com/unkos-dev/reverie/issues/1445)) ([6db251a](https://github.com/unkos-dev/reverie/commit/6db251a3ec56e1a0c0f5ad0d76a6dfee4bbeb2af))
* **renovate:** exempt digest updates from the release-age hold ([#1096](https://github.com/unkos-dev/reverie/issues/1096)) ([f4ea785](https://github.com/unkos-dev/reverie/commit/f4ea785e6dea66c6864d296d0d0705a80c970f6a))
* **settings:** require at least one accepted ingestion format ([#1411](https://github.com/unkos-dev/reverie/issues/1411)) ([b57524a](https://github.com/unkos-dev/reverie/commit/b57524ace4f2c1d75fe8592cc124364ba1cb2f9d))
* **shelves:** reject a reorder list that repeats an item ([#1218](https://github.com/unkos-dev/reverie/issues/1218)) ([7c6fa6a](https://github.com/unkos-dev/reverie/commit/7c6fa6a678547a09191b53703ec7d415fb51faf1))
* **storage:** preserve file ownership across publication and relocation ([#1275](https://github.com/unkos-dev/reverie/issues/1275)) ([a3c65e1](https://github.com/unkos-dev/reverie/commit/a3c65e193fb00535c7c9d53942f3188a97afa0ab))
* **test:** preserve nextest filter arguments ([#1348](https://github.com/unkos-dev/reverie/issues/1348)) ([4976813](https://github.com/unkos-dev/reverie/commit/49768131b64d17f4c8dd1bdbc56229890d33f4c5))
* **tests:** avoid blocking the migration test runtime ([#1432](https://github.com/unkos-dev/reverie/issues/1432)) ([b05d8d6](https://github.com/unkos-dev/reverie/commit/b05d8d6641cc87c88e851c06a1cac7b70d8a62a4))
* **theme:** avoid palette drift on version-only updates ([#1059](https://github.com/unkos-dev/reverie/issues/1059)) ([c0aa69b](https://github.com/unkos-dev/reverie/commit/c0aa69be9d855d745a88633851c1a69177296dc8))
* **tooling:** keep mise shims out of the doctor fixture ([#1329](https://github.com/unkos-dev/reverie/issues/1329)) ([abbcea4](https://github.com/unkos-dev/reverie/commit/abbcea4124f9fb17fec87302b8c1ced285b0ae72))
* **tooling:** remove local Kache requirements ([#1146](https://github.com/unkos-dev/reverie/issues/1146)) ([6bd0446](https://github.com/unkos-dev/reverie/commit/6bd0446d5d8baa07fa0efa2a9bc255f060462dca))
* **ui:** align lockup with canonical artwork ([#1262](https://github.com/unkos-dev/reverie/issues/1262)) ([9068c4c](https://github.com/unkos-dev/reverie/commit/9068c4c8c480bffb07aa10f7968495414f2f49ed))
* **ui:** send the user to sign in when a mutation answers 401 ([#1373](https://github.com/unkos-dev/reverie/issues/1373)) ([cfecdb5](https://github.com/unkos-dev/reverie/commit/cfecdb55b396f0539dca46ed2aedc7c60522c9d9))


### Performance

* **epub:** copy untouched entries verbatim during repack and trim zip to Deflate ([#1117](https://github.com/unkos-dev/reverie/issues/1117)) ([2e6cddd](https://github.com/unkos-dev/reverie/commit/2e6cddd839b724483706b70c0de354c7672749ce))


### Code Refactoring

* **auth:** limit API credentials to Reverie-issued tokens ([#1440](https://github.com/unkos-dev/reverie/issues/1440)) ([85cb619](https://github.com/unkos-dev/reverie/commit/85cb6190290730341a62e2bfef96f930e8a4ffed))

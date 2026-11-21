# Changelog

## 0.1.0 (2026-11-21)


### ⚠ BREAKING CHANGES

* **auth:** The API no longer accepts identity-provider access tokens, and the `REVERIE_RESOURCE_SERVER_*` settings are removed. API clients must use Reverie-issued scoped tokens instead. OIDC browser sign-in remains supported.

### Features

* **api:** add the authenticated GraphQL and Relay foundation ([#1849](https://github.com/unkos-dev/reverie/issues/1849)) ([f196487](https://github.com/unkos-dev/reverie/commit/f196487ee1df07ce79aa55ae61687f08aee45779))
* **api:** carry path-safe TypeID global IDs through browser routes ([#1911](https://github.com/unkos-dev/reverie/issues/1911)) ([1fce884](https://github.com/unkos-dev/reverie/commit/1fce884b9d15e628ea484d8e133487367413fd69))
* **api:** guarantee metadata formats with server-typed GraphQL scalars ([#1926](https://github.com/unkos-dev/reverie/issues/1926)) ([ea2cde5](https://github.com/unkos-dev/reverie/commit/ea2cde589d0fc260749f4e02459d4472e21aadcb))
* **api:** read book metadata as revision records so the store cannot mix revisions ([#1918](https://github.com/unkos-dev/reverie/issues/1918)) ([94e2b1b](https://github.com/unkos-dev/reverie/commit/94e2b1b65ed7e23858a3ccbef7d021ef80da9a49))
* **api:** return metadata mutation outcomes as typed union members ([#1921](https://github.com/unkos-dev/reverie/issues/1921)) ([7ec1e31](https://github.com/unkos-dev/reverie/commit/7ec1e319c268741c081042806591bf290ce21f9f))
* **auth:** let account holders choose their own passwords ([#1510](https://github.com/unkos-dev/reverie/issues/1510)) ([574ada6](https://github.com/unkos-dev/reverie/commit/574ada64a4f05abdd06fe195b95a82c14ff3aa69))
* **auth:** recover accounts through verified email ([#1540](https://github.com/unkos-dev/reverie/issues/1540)) ([f61ba6c](https://github.com/unkos-dev/reverie/commit/f61ba6c5591bb6a79e7cc9dbfd956c57a31d1801))
* **enrichment:** bound whole runs and rescue stale claims ([#1723](https://github.com/unkos-dev/reverie/issues/1723)) ([21160ba](https://github.com/unkos-dev/reverie/commit/21160ba1f4a7ba3bd170f720c8365abeff924190))
* **frontend:** add a persistent monochrome palette ([#1868](https://github.com/unkos-dev/reverie/issues/1868)) ([0d4bac5](https://github.com/unkos-dev/reverie/commit/0d4bac502b36775ce2b11ad6e0cb34d9f08a7bde))
* **frontend:** add the admin settings screen ([#1377](https://github.com/unkos-dev/reverie/issues/1377)) ([a81374d](https://github.com/unkos-dev/reverie/commit/a81374d870267f11f47ce29b526d450a18b1c0c7))
* **frontend:** establish the new palette and type foundation ([#1845](https://github.com/unkos-dev/reverie/issues/1845)) ([7231c32](https://github.com/unkos-dev/reverie/commit/7231c328afcff23254205573f6d5b2b042690e92))
* **frontend:** replace the left rail with a floating top bar ([#1851](https://github.com/unkos-dev/reverie/issues/1851)) ([6a4dd7f](https://github.com/unkos-dev/reverie/commit/6a4dd7fb8f74ce571e7470b81897656c6e3e8c46))
* **graphql:** expose the reader graph through shared operations ([#1862](https://github.com/unkos-dev/reverie/issues/1862)) ([3cb40b9](https://github.com/unkos-dev/reverie/commit/3cb40b9f7d254560f0f1947ff5fb3ae622d74ec3))
* **graphql:** preserve protected editing across transports ([#1878](https://github.com/unkos-dev/reverie/issues/1878)) ([9fd5fb3](https://github.com/unkos-dev/reverie/commit/9fd5fb3d14a761ed95eed2c7a61e3b71550e2db9))
* **ingestion:** expose admission control and runtime blockers ([#1459](https://github.com/unkos-dev/reverie/issues/1459)) ([132c3eb](https://github.com/unkos-dev/reverie/commit/132c3eb051179398e0eeee46d5cf38b689fcb11a))
* **ingestion:** list files skipped as duplicates ([#1487](https://github.com/unkos-dev/reverie/issues/1487)) ([25be5a9](https://github.com/unkos-dev/reverie/commit/25be5a92cd0c412676e56f675b56b3b14e5df697))
* **ingestion:** make import suspension visible ([#1461](https://github.com/unkos-dev/reverie/issues/1461)) ([85fff85](https://github.com/unkos-dev/reverie/commit/85fff85f59ebb7348d50f03627fa615fbff0b982))
* **ingestion:** retry failure groups without bypassing scheduling ([#1464](https://github.com/unkos-dev/reverie/issues/1464)) ([d9767e7](https://github.com/unkos-dev/reverie/commit/d9767e739db67bbedac7aa763b001b98669b6eda))
* **library:** keep filters beside the collection and size covers ([#1866](https://github.com/unkos-dev/reverie/issues/1866)) ([81e1ef4](https://github.com/unkos-dev/reverie/commit/81e1ef4c90a6f0ede753afac3436e85d2d0c7515))
* **library:** protect edit baselines and table Undo ([#1764](https://github.com/unkos-dev/reverie/issues/1764)) ([fffaafb](https://github.com/unkos-dev/reverie/commit/fffaafbea5376de4fbdd6298c86ad32a753682f7))
* **library:** rebuild the Library table view on the new visual system ([#1864](https://github.com/unkos-dev/reverie/issues/1864)) ([8200715](https://github.com/unkos-dev/reverie/commit/82007156db957dc9ca1a717aba102c7363ed83a1))
* **library:** render the Library table on React Aria for columns that fill and rows that fit ([#1800](https://github.com/unkos-dev/reverie/issues/1800)) ([e77316a](https://github.com/unkos-dev/reverie/commit/e77316ad235513239f36c70e34121a2d3ead4bf9))
* **library:** reorder authors without re-entering names ([#1807](https://github.com/unkos-dev/reverie/issues/1807)) ([61ebd23](https://github.com/unkos-dev/reverie/commit/61ebd2306c44bebd4a9f2759ed5a7d8d8609f593))
* **metadata:** preserve independent ISBNs in EPUB writeback ([#1709](https://github.com/unkos-dev/reverie/issues/1709)) ([a26cc94](https://github.com/unkos-dev/reverie/commit/a26cc948e4ee0a93cf9c776c66441a0adbaca5cb))
* **metadata:** preserve supplied ISBN values independently ([#1690](https://github.com/unkos-dev/reverie/issues/1690)) ([3e8f304](https://github.com/unkos-dev/reverie/commit/3e8f304d0951d47aa18897f98164423969099244))
* **metadata:** surface ISBN corrections and formatted partial matches ([#1716](https://github.com/unkos-dev/reverie/issues/1716)) ([719ae44](https://github.com/unkos-dev/reverie/commit/719ae44c216f54f2c4fc614c31b219072fa029df))
* **reader:** preserve browsing and editing context with Relay ([#1881](https://github.com/unkos-dev/reverie/issues/1881)) ([10304f9](https://github.com/unkos-dev/reverie/commit/10304f970ec52dc8e55a62d1d9937d01189ab769))
* **runtime:** recover failed background workers ([#1452](https://github.com/unkos-dev/reverie/issues/1452)) ([d06786e](https://github.com/unkos-dev/reverie/commit/d06786e9c9505edb19130caea1050c1ba3d18417))
* **shelves:** return shelf outcomes as typed results so the list updates without refetching ([#1913](https://github.com/unkos-dev/reverie/issues/1913)) ([ba4ae6e](https://github.com/unkos-dev/reverie/commit/ba4ae6ec248698e0e87706c34b48c16264383140))
* **ui:** restyle the shared controls to the new control language ([#1850](https://github.com/unkos-dev/reverie/issues/1850)) ([8f566f3](https://github.com/unkos-dev/reverie/commit/8f566f3c858777dc4a4add61ebba5eb35d8824bc))


### Bug Fixes

* **auth:** bound hashing before acquiring login connections ([#1504](https://github.com/unkos-dev/reverie/issues/1504)) ([f0e30d0](https://github.com/unkos-dev/reverie/commit/f0e30d05802eb80244265e557ee6c0be049f9e72))
* **auth:** prevent replay and competing sign-ins ([#1477](https://github.com/unkos-dev/reverie/issues/1477)) ([635f440](https://github.com/unkos-dev/reverie/commit/635f4400d9a0e672a59a2df1f3cc19460fbf2cf8))
* **auth:** prevent verification through account blocks ([#1500](https://github.com/unkos-dev/reverie/issues/1500)) ([89ad785](https://github.com/unkos-dev/reverie/commit/89ad785f1c81d7816330773bb45b5e21f1e1aecf))
* **auth:** return OIDC failures to sign-in recovery ([#1484](https://github.com/unkos-dev/reverie/issues/1484)) ([6618a19](https://github.com/unkos-dev/reverie/commit/6618a194f40dcf06967df4d990f420e154205b66))
* **backend:** clear Chrono and lettre dependency advisories ([#1900](https://github.com/unkos-dev/reverie/issues/1900)) ([14d4934](https://github.com/unkos-dev/reverie/commit/14d493400cd74ee5815b6e862929f878890228f7))
* **ci:** run CodeQL nightly and stop repeating jobs on main pushes ([#1813](https://github.com/unkos-dev/reverie/issues/1813)) ([82edef8](https://github.com/unkos-dev/reverie/commit/82edef88a9c12a235124bac7791aa1e79acb122c))
* **deps:** keep OIDC tests working with RSA 0.10 ([#1791](https://github.com/unkos-dev/reverie/issues/1791)) ([4ea74d5](https://github.com/unkos-dev/reverie/commit/4ea74d54ebedd551d2439c2b646d4af4ff1f56d1))
* **deps:** patch ctutils and accept assessed crypto-bigint risk ([#1570](https://github.com/unkos-dev/reverie/issues/1570)) ([5a0a745](https://github.com/unkos-dev/reverie/commit/5a0a745aefb3aebcf9f5d5553d00509cff247c2a))
* **deps:** prevent split Playwright versions and stalled dialog focus ([#1790](https://github.com/unkos-dev/reverie/issues/1790)) ([6235d32](https://github.com/unkos-dev/reverie/commit/6235d323933ec003093cec04000fc9a2c6363f0c))
* **deps:** reject distrusted certificate chains ([#1513](https://github.com/unkos-dev/reverie/issues/1513)) ([20d40ed](https://github.com/unkos-dev/reverie/commit/20d40ed8f02edc608f4b4da8c9d50d0f8fd51079))
* **deps:** resolve mise aqua tool updates from GitHub releases ([#1613](https://github.com/unkos-dev/reverie/issues/1613)) ([73b09ad](https://github.com/unkos-dev/reverie/commit/73b09ad513d8d126a38b5f0ed35208a71204e6f1))
* **deps:** update dependency @astrojs/starlight to ^0.43.0 ([#1884](https://github.com/unkos-dev/reverie/issues/1884)) ([6f6c793](https://github.com/unkos-dev/reverie/commit/6f6c79328193ec2b62b63bf641e2ef61fcfcea64))
* **deps:** update dependency react-data-grid to v7.0.0-beta.62 ([#1546](https://github.com/unkos-dev/reverie/issues/1546)) ([352a4ce](https://github.com/unkos-dev/reverie/commit/352a4ce96d38b0a141675465d8408e902b0bb883))
* **deps:** update dependency react-data-grid to v7.0.0-beta.63 ([#1617](https://github.com/unkos-dev/reverie/issues/1617)) ([079a2a4](https://github.com/unkos-dev/reverie/commit/079a2a425939b1db9774cc9f0b3cc29509824b25))
* **deps:** update react-data-grid to v7.0.0-beta.64 ([#1781](https://github.com/unkos-dev/reverie/issues/1781)) ([ce6eff7](https://github.com/unkos-dev/reverie/commit/ce6eff79e2eed723020767d7bf226b83f113f2bd))
* **deps:** update rust crate redactable to v0.14.1 ([#1537](https://github.com/unkos-dev/reverie/issues/1537)) ([4d4d9fa](https://github.com/unkos-dev/reverie/commit/4d4d9fafdaeadba013592964e94a34dd5ae778c4))
* **deps:** update rust crate redactable to v0.15.0 ([#1583](https://github.com/unkos-dev/reverie/issues/1583)) ([46c96f2](https://github.com/unkos-dev/reverie/commit/46c96f24f6596df07ca325eb386baf122c3c1abc))
* **deps:** update rust crate redactable to v0.15.2 ([#1600](https://github.com/unkos-dev/reverie/issues/1600)) ([1188523](https://github.com/unkos-dev/reverie/commit/118852393a63e0d61adb1e0350e015ad8a302bf3))
* **deps:** update rust crate resvg to 0.49 ([#1602](https://github.com/unkos-dev/reverie/issues/1602)) ([d48add7](https://github.com/unkos-dev/reverie/commit/d48add749564e68ceba20bee5b446deaa9da5a9d))
* **deps:** update rust crate resvg to 0.50 ([#1856](https://github.com/unkos-dev/reverie/issues/1856)) ([f593eee](https://github.com/unkos-dev/reverie/commit/f593eeebc9c684794f393eda68a7765da9f25242))
* **deps:** update rust crate zip to v9 ([#1463](https://github.com/unkos-dev/reverie/issues/1463)) ([a996473](https://github.com/unkos-dev/reverie/commit/a9964735cb11ddd608272188f2f13e70194e95bd))
* **frontend:** give surfaces a defined background ([#1854](https://github.com/unkos-dev/reverie/issues/1854)) ([fafd318](https://github.com/unkos-dev/reverie/commit/fafd31895c0c3a05ee3c2641482a655a3183f18c))
* **git:** make the worktree recipes work from any worktree ([#1803](https://github.com/unkos-dev/reverie/issues/1803)) ([09004a4](https://github.com/unkos-dev/reverie/commit/09004a4133505a6774992b7610885f98d0b9a58b))
* **ingestion:** keep concurrent bulk retries from resetting the same inputs ([#1703](https://github.com/unkos-dev/reverie/issues/1703)) ([fe3ff3b](https://github.com/unkos-dev/reverie/commit/fe3ff3b29224172f9260cbbb4b5359813d730a6a))
* **ingestion:** reject corrupt EPUBs instead of importing them ([#1398](https://github.com/unkos-dev/reverie/issues/1398)) ([fa6f895](https://github.com/unkos-dev/reverie/commit/fa6f895bac36bc8733b4f6ec2ac2cd417156e668))
* **ingestion:** release test ownership before the next scan ([#1444](https://github.com/unkos-dev/reverie/issues/1444)) ([79effc8](https://github.com/unkos-dev/reverie/commit/79effc857202635f0fcdf15bdda4a722b441cb51))
* **metadata:** bind validators to canonical change identity ([#1760](https://github.com/unkos-dev/reverie/issues/1760)) ([bbb7c52](https://github.com/unkos-dev/reverie/commit/bbb7c528a819559b6830356967cd8b089fcd51d3))
* **metadata:** honour edition locks on manual writes ([#1759](https://github.com/unkos-dev/reverie/issues/1759)) ([3763bc2](https://github.com/unkos-dev/reverie/commit/3763bc288c50a3a35e90169fac8f1fe6b8cb8c33))
* **metadata:** keep clear confirmation interactive on submission ([#1664](https://github.com/unkos-dev/reverie/issues/1664)) ([e865157](https://github.com/unkos-dev/reverie/commit/e865157b4c24de3036a897a7841ff631ac8cd56f))
* **metadata:** keep unrelated identifiers out of ISBN extraction ([#1702](https://github.com/unkos-dev/reverie/issues/1702)) ([9fa46f2](https://github.com/unkos-dev/reverie/commit/9fa46f2b5fa44db9a716db9b551ddd8e6bf0b3ab))
* **metadata:** preserve provenance through exact field undo ([#1762](https://github.com/unkos-dev/reverie/issues/1762)) ([e2d3ed1](https://github.com/unkos-dev/reverie/commit/e2d3ed17036c857fc7a2441846d595eb826b94a6))
* **metadata:** preserve publication date precision ([#1663](https://github.com/unkos-dev/reverie/issues/1663)) ([898f4e8](https://github.com/unkos-dev/reverie/commit/898f4e8dad396821b91e4161eb0ebfc7520d7048))
* **metadata:** preserve shared vocabulary spelling ([#1758](https://github.com/unkos-dev/reverie/issues/1758)) ([9c539e9](https://github.com/unkos-dev/reverie/commit/9c539e9953f3f3d0e09f0deaedf5dd3df973c99e))
* **metadata:** recognise equivalent Unicode observations ([#1822](https://github.com/unkos-dev/reverie/issues/1822)) ([75e0ffb](https://github.com/unkos-dev/reverie/commit/75e0ffb0495dab10dce3a7d45a89b83ba5988e07))
* **opds:** preserve challenges for configured realms ([#1821](https://github.com/unkos-dev/reverie/issues/1821)) ([e59b061](https://github.com/unkos-dev/reverie/commit/e59b06156fe1dd02632cf5e780f92467f7daa72e))
* **openapi:** publish existing request bounds ([#1445](https://github.com/unkos-dev/reverie/issues/1445)) ([6db251a](https://github.com/unkos-dev/reverie/commit/6db251a3ec56e1a0c0f5ad0d76a6dfee4bbeb2af))
* **scripts:** exit with the wrapped command's status from the Postgres provisioner ([#1552](https://github.com/unkos-dev/reverie/issues/1552)) ([a408a02](https://github.com/unkos-dev/reverie/commit/a408a02ef5d9a0aaea2b995cf03a6317a47560ea))
* **series:** keep child catalogues within visible books ([#1623](https://github.com/unkos-dev/reverie/issues/1623)) ([f07c30a](https://github.com/unkos-dev/reverie/commit/f07c30a0f7fa68b849a23e880cd99975ac2dee09))
* **settings:** require at least one accepted ingestion format ([#1411](https://github.com/unkos-dev/reverie/issues/1411)) ([b57524a](https://github.com/unkos-dev/reverie/commit/b57524ace4f2c1d75fe8592cc124364ba1cb2f9d))
* **tooling:** preserve local plans before linking shared storage ([#1625](https://github.com/unkos-dev/reverie/issues/1625)) ([0d83e0f](https://github.com/unkos-dev/reverie/commit/0d83e0fa8baa1884238ed8dd6cfc1c24cd51c636))
* **tooling:** provision externally created worktrees consistently ([#1606](https://github.com/unkos-dev/reverie/issues/1606)) ([cec97b1](https://github.com/unkos-dev/reverie/commit/cec97b1593407f33760b7e40a50bbd4052d1cf1f))
* **ui:** preserve native selection and focus composition ([#1511](https://github.com/unkos-dev/reverie/issues/1511)) ([31b14e1](https://github.com/unkos-dev/reverie/commit/31b14e12002f00b8066c7b2a0b5b19c67339a651))
* **users:** synchronise child status with role changes ([#1507](https://github.com/unkos-dev/reverie/issues/1507)) ([6c64fe4](https://github.com/unkos-dev/reverie/commit/6c64fe4270a89a30420ed5885c9f480cd96fa041))


### Code Refactoring

* **auth:** limit API credentials to Reverie-issued tokens ([#1440](https://github.com/unkos-dev/reverie/issues/1440)) ([85cb619](https://github.com/unkos-dev/reverie/commit/85cb6190290730341a62e2bfef96f930e8a4ffed))

# Graph Report - /home/coder/reverie  (2026-04-28)

## Corpus Check
- 163 files · ~101,659 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1529 nodes · 4135 edges · 81 communities detected
- Extraction: 77% EXTRACTED · 23% INFERRED · 0% AMBIGUOUS · INFERRED: 951 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Backend Test Harness|Backend Test Harness]]
- [[_COMMUNITY_OPDS Catalog Feeds|OPDS Catalog Feeds]]
- [[_COMMUNITY_Agent Toolset (docs)|Agent Toolset (docs)]]
- [[_COMMUNITY_Ingestion File Pipeline|Ingestion File Pipeline]]
- [[_COMMUNITY_Cover Image Cache|Cover Image Cache]]
- [[_COMMUNITY_Field Locking|Field Locking]]
- [[_COMMUNITY_EPUB Extraction Layers|EPUB Extraction Layers]]
- [[_COMMUNITY_Dry-Run Preview|Dry-Run Preview]]
- [[_COMMUNITY_Metadata Sanitiser|Metadata Sanitiser]]
- [[_COMMUNITY_CSP & Error Headers|CSP & Error Headers]]
- [[_COMMUNITY_API Entry Points|API Entry Points]]
- [[_COMMUNITY_Cover Source Downloads|Cover Source Downloads]]
- [[_COMMUNITY_Encoding & Model Enums|Encoding & Model Enums]]
- [[_COMMUNITY_OPF Document Rewrite|OPF Document Rewrite]]
- [[_COMMUNITY_Enrichment API Routes|Enrichment API Routes]]
- [[_COMMUNITY_Work & Path Materialisation|Work & Path Materialisation]]
- [[_COMMUNITY_API Response Cache|API Response Cache]]
- [[_COMMUNITY_SSRF Allowlist|SSRF Allowlist]]
- [[_COMMUNITY_Auth Backends|Auth Backends]]
- [[_COMMUNITY_OIDC & Theme Cookies|OIDC & Theme Cookies]]
- [[_COMMUNITY_Frontend Theming|Frontend Theming]]
- [[_COMMUNITY_Google Books Provider|Google Books Provider]]
- [[_COMMUNITY_Canonical Value Hashing|Canonical Value Hashing]]
- [[_COMMUNITY_Frontend Dist Validation|Frontend Dist Validation]]
- [[_COMMUNITY_ISBN Parsing|ISBN Parsing]]
- [[_COMMUNITY_Community 25|Community 25]]
- [[_COMMUNITY_Community 26|Community 26]]
- [[_COMMUNITY_Community 27|Community 27]]
- [[_COMMUNITY_Community 28|Community 28]]
- [[_COMMUNITY_Community 29|Community 29]]
- [[_COMMUNITY_Community 30|Community 30]]
- [[_COMMUNITY_Community 31|Community 31]]
- [[_COMMUNITY_Community 32|Community 32]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]
- [[_COMMUNITY_Community 35|Community 35]]
- [[_COMMUNITY_Community 36|Community 36]]
- [[_COMMUNITY_Community 37|Community 37]]
- [[_COMMUNITY_Community 38|Community 38]]
- [[_COMMUNITY_Community 39|Community 39]]
- [[_COMMUNITY_Community 40|Community 40]]
- [[_COMMUNITY_Community 41|Community 41]]
- [[_COMMUNITY_Community 42|Community 42]]
- [[_COMMUNITY_Community 43|Community 43]]
- [[_COMMUNITY_Community 44|Community 44]]
- [[_COMMUNITY_Community 45|Community 45]]
- [[_COMMUNITY_Community 46|Community 46]]
- [[_COMMUNITY_Community 47|Community 47]]
- [[_COMMUNITY_Community 48|Community 48]]
- [[_COMMUNITY_Community 49|Community 49]]
- [[_COMMUNITY_Community 50|Community 50]]
- [[_COMMUNITY_Community 51|Community 51]]
- [[_COMMUNITY_Community 52|Community 52]]
- [[_COMMUNITY_Community 53|Community 53]]
- [[_COMMUNITY_Community 54|Community 54]]
- [[_COMMUNITY_Community 55|Community 55]]
- [[_COMMUNITY_Community 56|Community 56]]
- [[_COMMUNITY_Community 57|Community 57]]
- [[_COMMUNITY_Community 58|Community 58]]
- [[_COMMUNITY_Community 59|Community 59]]
- [[_COMMUNITY_Community 60|Community 60]]
- [[_COMMUNITY_Community 61|Community 61]]
- [[_COMMUNITY_Community 62|Community 62]]
- [[_COMMUNITY_Community 63|Community 63]]
- [[_COMMUNITY_Community 64|Community 64]]
- [[_COMMUNITY_Community 65|Community 65]]
- [[_COMMUNITY_Community 66|Community 66]]
- [[_COMMUNITY_Community 67|Community 67]]
- [[_COMMUNITY_Community 68|Community 68]]
- [[_COMMUNITY_Community 69|Community 69]]
- [[_COMMUNITY_Community 70|Community 70]]
- [[_COMMUNITY_Community 71|Community 71]]
- [[_COMMUNITY_Community 72|Community 72]]
- [[_COMMUNITY_Community 73|Community 73]]
- [[_COMMUNITY_Community 114|Community 114]]
- [[_COMMUNITY_Community 115|Community 115]]
- [[_COMMUNITY_Community 116|Community 116]]
- [[_COMMUNITY_Community 117|Community 117]]
- [[_COMMUNITY_Community 118|Community 118]]
- [[_COMMUNITY_Community 119|Community 119]]
- [[_COMMUNITY_Community 120|Community 120]]

## God Nodes (most connected - your core abstractions)
1. `ingestion_pool_for()` - 88 edges
2. `write()` - 51 edges
3. `app_pool_for()` - 47 edges
4. `read()` - 33 edges
5. `run_once()` - 27 edges
6. `server_with_opds_enabled()` - 26 edges
7. `finish()` - 25 edges
8. `create_admin_and_basic_auth()` - 24 edges
9. `run_once()` - 23 edges
10. `test_config()` - 23 edges

## Surprising Connections (you probably didn't know these)
- `Conduct Report Channel (Private Advisory)` --semantically_similar_to--> `Safe Harbor for Good-Faith Research`  [INFERRED] [semantically similar]
  CODE_OF_CONDUCT.md → SECURITY.md
- `FFL Clause-02 Acceptance` --semantically_similar_to--> `AGPL-3.0 License`  [INFERRED] [semantically similar]
  frontend/public/fonts/fontshare/README.md → README.md
- `Renovate Dependency Bot` --references--> `Vulnerability Reporting (GitHub Private Advisory)`  [INFERRED]
  CONTRIBUTING.md → SECURITY.md
- `Reverie (project)` --references--> `Code of Conduct Pledge`  [INFERRED]
  README.md → CODE_OF_CONDUCT.md
- `Conduct Report Channel (Private Advisory)` --shares_data_with--> `Vulnerability Reporting (GitHub Private Advisory)`  [EXTRACTED]
  CODE_OF_CONDUCT.md → SECURITY.md

## Hyperedges (group relationships)
- **Theme Cookie Cross-Stack Contract** — visual_identity_cookie_three_place_rule, visual_identity_cookie_attribute_parity, content_security_policy_reverie_theme_cookie, visual_identity_fouc_mechanism, schema_users_table [EXTRACTED 0.95]
- **RLS Access Control Flow on manifestations** — schema_rls, schema_session_var_contract, schema_reverie_app_role, schema_reverie_readonly_role, schema_reverie_ingestion_role, schema_user_role_enum [EXTRACTED 0.95]
- **Reverie Default Security Header Set (per response)** — content_security_policy_default_headers, content_security_policy_html_csp, content_security_policy_api_csp, content_security_policy_hsts_optin, content_security_policy_csp_reporting, content_security_policy_security_grade_target [EXTRACTED 0.90]

## Communities

### Community 0 - "Backend Test Harness"
Cohesion: 0.07
Nodes (94): adult_shelf_scoped_feed(), basic_only_db_failure_returns_500_not_challenge(), child_sees_only_whitelisted_manifestations(), cover_cache_populates_and_serves(), cross_user_shelf_returns_404(), download_streams_and_path_traversal_403(), empty_library_has_no_next_link(), exact_page_size_has_no_next_link() (+86 more)

### Community 1 - "OPDS Catalog Feeds"
Cohesion: 0.06
Nodes (82): finish(), Cursor, CursorError, encode_parse_roundtrip(), rejects_bad_timestamp(), rejects_bad_uuid(), rejects_invalid_base64(), rejects_missing_delimiter() (+74 more)

### Community 2 - "Agent Toolset (docs)"
Cohesion: 0.02
Nodes (105): Active MCPs (sdl-mcp, docmost, context7), block-no-verify.js hook, config-protection.js hook, crosscheck custom skill, dotfiles repository (junkovich/dotfiles), ECC Agents (symlinked from ecc-src), ecc-keep.yaml keep-list, ECC Skills (symlinked from ecc-src) (+97 more)

### Community 3 - "Ingestion File Pipeline"
Cohesion: 0.08
Nodes (54): write(), cleanup_batch(), cleanup_missing_file_is_ok(), cleanup_preserves_non_empty_dirs(), cleanup_removes_files_and_empty_dirs(), CleanupResult, copy_verified(), copy_verified_detects_hash_mismatch() (+46 more)

### Community 4 - "Cover Image Cache"
Cohesion: 0.1
Nodes (51): CoverCache, cache_root(), ext_for_format(), get_or_create(), writeback_pool_for(), compute_hex_sha256(), extract_opf_path(), extract_opf_path_reads_full_path_attribute() (+43 more)

### Community 5 - "Field Locking"
Cohesion: 0.1
Nodes (44): a_user(), EntityType, is_locked(), is_locked_tx(), lock(), lock_unlock_roundtrip(), setup_fixture(), unlock() (+36 more)

### Community 6 - "EPUB Extraction Layers"
Cohesion: 0.09
Nodes (43): extract_cover_bytes(), extract_opf_path(), make_handle_with_entries(), missing_container_with_opf_emits_repaired(), unsafe_opf_path_in_container_xml_emits_irrecoverable(), valid_container_returns_opf_path(), validate(), find_cover_href() (+35 more)

### Community 7 - "Dry-Run Preview"
Cohesion: 0.11
Nodes (40): DryRunDiff, FieldChange, load_existing_pending_readonly(), preview(), SourceFailureSummary, apply_field(), build_sources(), cache_all() (+32 more)

### Community 8 - "Metadata Sanitiser"
Cohesion: 0.1
Nodes (40): date_parsing_variants(), empty_opf(), extract(), extract_full_metadata(), extract_minimal_metadata(), ExtractedCreator, ExtractedMetadata, generate_sort_name() (+32 more)

### Community 9 - "CSP & Error Headers"
Cohesion: 0.13
Nodes (40): api_404_with_csp(), api_csp_layer(), api_typo_returns_json_404_with_api_csp(), assets_served_with_html_csp(), attach_api_csp(), attach_html_csp(), auth_typo_returns_json_404_with_api_csp(), composite_fallback() (+32 more)

### Community 10 - "API Entry Points"
Cohesion: 0.11
Nodes (30): api_router(), opds_router(), serve_cover(), covers_router(), router_enabled(), build_opensearch_xml(), build_response(), library_opensearch() (+22 more)

### Community 11 - "Cover Source Downloads"
Cohesion: 0.15
Nodes (34): client(), content_type_jpeg_but_png_bytes_rejected(), CoverArtifact, CoverError, CoverFormat, default_config(), dimensions_too_small_rejected(), download() (+26 more)

### Community 12 - "Encoding & Model Enums"
Cohesion: 0.09
Nodes (25): detect_declared_encoding(), latin1_declared_non_utf8_bytes_emits_repaired(), non_utf8_no_declaration_emits_degraded(), validate(), validate_xhtml_document(), validate_xml_parse(), as_str_matches_serde_lowercase(), IngestionStatus (+17 more)

### Community 13 - "OPF Document Rewrite"
Cohesion: 0.17
Nodes (37): attr_value(), attr_value_as_string(), format_index(), has_attr(), is_isbn_identifier(), local_name(), read_all_events(), sample_epub2() (+29 more)

### Community 14 - "Enrichment API Routes"
Cohesion: 0.13
Nodes (32): dry_run(), dry_run_requires_auth(), router(), status(), status_requires_auth(), StatusSummary, trigger(), trigger_requires_auth() (+24 more)

### Community 15 - "Work & Path Materialisation"
Cohesion: 0.17
Nodes (32): commit_ingest(), create_stub(), find_or_create(), find_or_create_author(), find_or_create_deduplicates_authors(), find_or_create_deduplicates_series(), find_or_create_matches_by_isbn(), find_or_create_new_work() (+24 more)

### Community 16 - "API Response Cache"
Cohesion: 0.17
Nodes (29): ApiCacheKind, CachedResponse, CacheTtls, distinct_kinds_get_distinct_expirations(), expired_entry_returns_none(), isbn10_and_isbn13_dedupe_via_lookup_key(), kind_from_str(), read() (+21 more)

### Community 17 - "SSRF Allowlist"
Cohesion: 0.14
Nodes (30): api_client(), cgnat_denied(), cover_client(), HopError, ip_is_denied(), ipv4_mapped_private_denied(), ipv4_mapped_public_allowed(), link_local_denied() (+22 more)

### Community 18 - "Auth Backends"
Cohesion: 0.1
Nodes (15): AuthBackend, OidcCredentials, BasicOnly, CurrentUser, verify_basic(), find_by_id(), find_by_oidc_subject(), role_decode_fails_for_unknown_db_variant() (+7 more)

### Community 19 - "OIDC & Theme Cookies"
Cohesion: 0.16
Nodes (17): exchange_http_client(), http_client(), init_oidc_client(), set_theme_cookie(), set_theme_cookie_writes_canonical_attributes(), callback(), login(), adapter_disabled_without_token() (+9 more)

### Community 20 - "Frontend Theming"
Cohesion: 0.1
Nodes (13): ThemeSwitcher(), App(), readThemeCookie(), writeThemeCookie(), applyEffective(), deriveInitialState(), resolveEffective(), installMatchMedia() (+5 more)

### Community 21 - "Google Books Provider"
Cohesion: 0.16
Nodes (16): ctx(), empty_items_returns_empty(), GoogleBooks, isbn_happy_path(), limiter(), map_volumes(), rate_limited_returns_with_retry_after(), sample_volume() (+8 more)

### Community 22 - "Canonical Value Hashing"
Cohesion: 0.23
Nodes (19): canonical_json(), different_values_hash_differently(), list_field_order_insensitive(), non_list_field_order_sensitive(), normalise(), normalise_item(), object_key_order_irrelevant(), pub_date_normalised() (+11 more)

### Community 23 - "Frontend Dist Validation"
Cohesion: 0.29
Nodes (20): dir_not_found(), DistValidationError, happy_path_one_hash(), happy_path_two_hashes_all_algos(), hash_regex(), index_html_missing(), make_valid_dist(), not_a_directory() (+12 more)

### Community 24 - "ISBN Parsing"
Cohesion: 0.26
Nodes (19): isbn10_invalid(), isbn10_to_13_conversion(), isbn10_to_isbn13(), isbn10_valid(), isbn10_with_x(), isbn10_x_check_digit_parsed(), isbn13_invalid(), isbn13_to_10_conversion() (+11 more)

### Community 25 - "Community 25"
Cohesion: 0.29
Nodes (18): attr_str(), CoverPlan, epub2_opf_no_cover(), epub2_opf_with_cover(), epub3_opf_no_cover(), epub3_opf_with_cover(), insert_opf_cover(), local_name() (+10 more)

### Community 26 - "Community 26"
Cohesion: 0.32
Nodes (16): all_autofill_fields_recognised(), all_propose_fields_recognised(), autofill_agreement_still_applies_when_empty(), autofill_canonical_already_set_stages(), autofill_disagreement_downgrades_to_stage(), autofill_empty_canonical_applies(), decide(), Decision (+8 more)

### Community 27 - "Community 27"
Cohesion: 0.35
Nodes (13): invalid_isbn_returns_none(), isbn10_and_isbn13_same_book_produce_same_key(), isbn_key(), isbn_key_with_hyphens(), normalise_author(), normalise_text(), title_author_key(), title_author_key_accented_chars_stripped_to_space() (+5 more)

### Community 28 - "Community 28"
Cohesion: 0.38
Nodes (11): claim_next(), insert_queue_fixture(), mark_complete(), mark_failed(), max_attempts_transitions_to_skipped(), retry_backoff_window_blocks_then_releases(), revert_in_progress(), shutdown_reverts_in_progress_to_pending() (+3 more)

### Community 29 - "Community 29"
Cohesion: 0.35
Nodes (11): ascii_fallback(), ascii_fallback_empty_falls_back_to_uuid(), ascii_fallback_strips_non_ascii(), canonicalise_file_for_download(), content_disposition(), content_disposition_format(), download_epub(), rfc5987_encode() (+3 more)

### Community 30 - "Community 30"
Cohesion: 0.38
Nodes (11): agreement_boost(), agreement_boost_boundaries(), base_source(), googlebooks_title_quorum0(), hardcover_isbn_quorum3_clamped_to_ceiling(), manual_isbn_quorum1_is_one(), match_modifier(), openlibrary_isbn_quorum3_under_ceiling() (+3 more)

### Community 31 - "Community 31"
Cohesion: 0.4
Nodes (10): CoverSize, decoded_long_edge(), make_jpeg(), make_png(), preserves_format_png(), rejects_unsupported_format(), resize_cover(), resizes_to_full_cap() (+2 more)

### Community 32 - "Community 32"
Cohesion: 0.5
Nodes (11): case_insensitive_extension(), custom_priority_pdf_first(), epub_beats_pdf_same_stem(), files_with_no_extension_ignored(), multiple_titles(), no_matching_format(), priority(), same_stem_different_dirs_not_grouped() (+3 more)

### Community 33 - "Community 33"
Cohesion: 0.47
Nodes (9): create(), find_by_batch(), IngestionJob, job_lifecycle(), job_skipped_and_failed(), mark_complete(), mark_failed(), mark_running() (+1 more)

### Community 34 - "Community 34"
Cohesion: 0.43
Nodes (6): cn(), Sheet(), SheetClose(), SheetDescription(), SheetPortal(), SheetTrigger()

### Community 35 - "Community 35"
Cohesion: 0.48
Nodes (5): cn(), Dialog(), DialogClose(), DialogPortal(), DialogTrigger()

### Community 36 - "Community 36"
Cohesion: 0.48
Nodes (5): Select(), SelectContent(), SelectGroup(), SelectTrigger(), SelectValue()

### Community 37 - "Community 37"
Cohesion: 0.53
Nodes (4): cn(), Table(), TableBody(), TableHeader()

### Community 38 - "Community 38"
Cohesion: 0.33
Nodes (6): Contributor Covenant v2.0, Correction (Tier 1), Enforcement Guidelines Ladder, Permanent Ban (Tier 4), Temporary Ban (Tier 3), Warning (Tier 2)

### Community 39 - "Community 39"
Cohesion: 0.6
Nodes (3): health(), ready(), router()

### Community 40 - "Community 40"
Cohesion: 0.6
Nodes (3): fakeResolvedConfig(), getHandler(), projectWithFouc()

### Community 41 - "Community 41"
Cohesion: 0.6
Nodes (3): AlertDialog(), AlertDialogCancel(), cn()

### Community 42 - "Community 42"
Cohesion: 0.6
Nodes (3): Popover(), PopoverDescription(), PopoverTrigger()

### Community 43 - "Community 43"
Cohesion: 0.6
Nodes (3): cn(), CommandGroup(), CommandItem()

### Community 44 - "Community 44"
Cohesion: 0.4
Nodes (5): --color-fg-faint Decorative-Only Rule, Light-Theme Accent axe Deviation, Reverie Gold Accent (#C9A961 dark / #8E6F38 light), State Without Hue Invariant, Warm Neutrals (Ink/Cream/Parchment)

### Community 45 - "Community 45"
Cohesion: 0.67
Nodes (2): fetchMe(), patchTheme()

### Community 46 - "Community 46"
Cohesion: 0.67
Nodes (2): cn(), DropdownMenu()

### Community 47 - "Community 47"
Cohesion: 0.67
Nodes (2): AvatarBadge(), cn()

### Community 48 - "Community 48"
Cohesion: 0.67
Nodes (2): CardAction(), cn()

### Community 49 - "Community 49"
Cohesion: 0.67
Nodes (2): ScrollArea(), ScrollBar()

### Community 50 - "Community 50"
Cohesion: 0.67
Nodes (2): runFouc(), stubMatchMedia()

### Community 51 - "Community 51"
Cohesion: 0.67
Nodes (2): RadioGroup(), RadioGroupItem()

### Community 52 - "Community 52"
Cohesion: 0.67
Nodes (2): cn(), InputGroupText()

### Community 53 - "Community 53"
Cohesion: 0.5
Nodes (4): Branch Prefix Convention, Conventional Commits Convention, Pull Request Process, Mandatory TDD Policy

### Community 54 - "Community 54"
Cohesion: 0.67
Nodes (1): AppState

### Community 55 - "Community 55"
Cohesion: 0.67
Nodes (1): WritebackError

### Community 56 - "Community 56"
Cohesion: 0.67
Nodes (1): CoverError

### Community 57 - "Community 57"
Cohesion: 0.67
Nodes (1): manualChunks()

### Community 58 - "Community 58"
Cohesion: 0.67
Nodes (1): cspHashPlugin()

### Community 59 - "Community 59"
Cohesion: 0.67
Nodes (1): cn()

### Community 60 - "Community 60"
Cohesion: 0.67
Nodes (1): Lockup()

### Community 61 - "Community 61"
Cohesion: 0.67
Nodes (1): cn()

### Community 62 - "Community 62"
Cohesion: 0.67
Nodes (1): Switch()

### Community 63 - "Community 63"
Cohesion: 0.67
Nodes (1): Badge()

### Community 64 - "Community 64"
Cohesion: 0.67
Nodes (1): Skeleton()

### Community 65 - "Community 65"
Cohesion: 0.67
Nodes (1): cn()

### Community 66 - "Community 66"
Cohesion: 0.67
Nodes (1): cn()

### Community 67 - "Community 67"
Cohesion: 0.67
Nodes (1): Label()

### Community 68 - "Community 68"
Cohesion: 0.67
Nodes (1): cn()

### Community 69 - "Community 69"
Cohesion: 0.67
Nodes (1): Input()

### Community 70 - "Community 70"
Cohesion: 0.67
Nodes (1): TooltipContent()

### Community 71 - "Community 71"
Cohesion: 0.67
Nodes (1): Separator()

### Community 72 - "Community 72"
Cohesion: 0.67
Nodes (1): Checkbox()

### Community 73 - "Community 73"
Cohesion: 1.0
Nodes (2): series_works.position NUMERIC, series table (self-referential)

### Community 114 - "Community 114"
Cohesion: 1.0
Nodes (1): Version 0.0.0

### Community 115 - "Community 115"
Cohesion: 1.0
Nodes (1): metadata_versions table

### Community 116 - "Community 116"
Cohesion: 1.0
Nodes (1): shelves table

### Community 117 - "Community 117"
Cohesion: 1.0
Nodes (1): writeback_jobs table

### Community 118 - "Community 118"
Cohesion: 1.0
Nodes (1): ingestion_jobs table

### Community 119 - "Community 119"
Cohesion: 1.0
Nodes (1): pgvector (reserved)

### Community 120 - "Community 120"
Cohesion: 1.0
Nodes (1): Vite + React + TypeScript Template README

## Knowledge Gaps
- **59 isolated node(s):** `Encouraged Behaviors`, `Restricted Behaviors`, `Correction (Tier 1)`, `Warning (Tier 2)`, `Temporary Ban (Tier 3)` (+54 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `Community 45`** (4 nodes): `api.ts`, `api.ts`, `fetchMe()`, `patchTheme()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 46`** (4 nodes): `dropdown-menu.tsx`, `dropdown-menu.tsx`, `cn()`, `DropdownMenu()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 47`** (4 nodes): `avatar.tsx`, `avatar.tsx`, `AvatarBadge()`, `cn()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 48`** (4 nodes): `card.tsx`, `card.tsx`, `CardAction()`, `cn()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 49`** (4 nodes): `scroll-area.tsx`, `scroll-area.tsx`, `ScrollArea()`, `ScrollBar()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 50`** (4 nodes): `runFouc()`, `stubMatchMedia()`, `fouc.test.ts`, `fouc.test.ts`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 51`** (4 nodes): `radio-group.tsx`, `radio-group.tsx`, `RadioGroup()`, `RadioGroupItem()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 52`** (4 nodes): `input-group.tsx`, `input-group.tsx`, `cn()`, `InputGroupText()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 54`** (3 nodes): `state.rs`, `state.rs`, `AppState`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 55`** (3 nodes): `error.rs`, `error.rs`, `WritebackError`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 56`** (3 nodes): `error.rs`, `CoverError`, `error.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 57`** (3 nodes): `manualChunks()`, `vite.config.ts`, `vite.config.ts`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 58`** (3 nodes): `csp-hash.ts`, `csp-hash.ts`, `cspHashPlugin()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 59`** (3 nodes): `utils.ts`, `utils.ts`, `cn()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 60`** (3 nodes): `Lockup()`, `Lockup.tsx`, `Lockup.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 61`** (3 nodes): `tabs.tsx`, `tabs.tsx`, `cn()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 62`** (3 nodes): `switch.tsx`, `switch.tsx`, `Switch()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 63`** (3 nodes): `badge.tsx`, `badge.tsx`, `Badge()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 64`** (3 nodes): `skeleton.tsx`, `skeleton.tsx`, `Skeleton()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 65`** (3 nodes): `textarea.tsx`, `textarea.tsx`, `cn()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 66`** (3 nodes): `field.tsx`, `field.tsx`, `cn()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 67`** (3 nodes): `label.tsx`, `label.tsx`, `Label()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 68`** (3 nodes): `button.tsx`, `button.tsx`, `cn()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 69`** (3 nodes): `input.tsx`, `input.tsx`, `Input()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 70`** (3 nodes): `tooltip.tsx`, `tooltip.tsx`, `TooltipContent()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 71`** (3 nodes): `separator.tsx`, `separator.tsx`, `Separator()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 72`** (3 nodes): `checkbox.tsx`, `checkbox.tsx`, `Checkbox()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 73`** (2 nodes): `series_works.position NUMERIC`, `series table (self-referential)`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 114`** (1 nodes): `Version 0.0.0`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 115`** (1 nodes): `metadata_versions table`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 116`** (1 nodes): `shelves table`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 117`** (1 nodes): `writeback_jobs table`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 118`** (1 nodes): `ingestion_jobs table`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 119`** (1 nodes): `pgvector (reserved)`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 120`** (1 nodes): `Vite + React + TypeScript Template README`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `run_once()` connect `Dry-Run Preview` to `Backend Test Harness`, `Cover Image Cache`, `Field Locking`, `Work & Path Materialisation`, `SSRF Allowlist`, `Canonical Value Hashing`, `Community 26`, `Community 30`?**
  _High betweenness centrality (0.060) - this node is a cross-community bridge._
- **Why does `ingestion_pool_for()` connect `Backend Test Harness` to `Community 33`, `Ingestion File Pipeline`, `Cover Image Cache`, `Field Locking`, `Dry-Run Preview`, `Work & Path Materialisation`, `API Response Cache`, `Canonical Value Hashing`, `Community 28`?**
  _High betweenness centrality (0.040) - this node is a cross-community bridge._
- **Why does `main()` connect `API Entry Points` to `Backend Test Harness`, `OPDS Catalog Feeds`, `Ingestion File Pipeline`, `Field Locking`, `Encoding & Model Enums`, `OIDC & Theme Cookies`, `Frontend Dist Validation`, `Community 28`?**
  _High betweenness centrality (0.035) - this node is a cross-community bridge._
- **Are the 85 inferred relationships involving `ingestion_pool_for()` (e.g. with `accept_admin_writes_canonical_title()` and `reject_admin_marks_version_rejected()`) actually correct?**
  _`ingestion_pool_for()` has 85 INFERRED edges - model-reasoned connections that need verification._
- **Are the 43 inferred relationships involving `write()` (e.g. with `insert_epub_manifestation()` and `download_streams_and_path_traversal_403()`) actually correct?**
  _`write()` has 43 INFERRED edges - model-reasoned connections that need verification._
- **Are the 44 inferred relationships involving `app_pool_for()` (e.g. with `accept_admin_writes_canonical_title()` and `reject_admin_marks_version_rejected()`) actually correct?**
  _`app_pool_for()` has 44 INFERRED edges - model-reasoned connections that need verification._
- **Are the 26 inferred relationships involving `read()` (e.g. with `spa_fallback_response()` and `validate_frontend_dist()`) actually correct?**
  _`read()` has 26 INFERRED edges - model-reasoned connections that need verification._
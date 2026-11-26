/**
 * @generated SignedSource<<8a5978fb19e074e92408f1d69db919d0>>
 * @relayHash 117e217a087aac87398542f3c70501b2
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 913627a461d124019b070e057a2056bfdd6c1a44298add9b61d7abc6d9b4e06c

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type Direction = "ASC" | "DESC" | "%future added value";
export type ReadingStatusFilter =
  | "ABANDONED"
  | "FINISHED"
  | "ON_HOLD"
  | "READING"
  | "UNREAD"
  | "WANT_TO_READ"
  | "%future added value";
export type SortField =
  | "AUTHOR"
  | "CREATED_AT"
  | "ISBN_13"
  | "PAGES"
  | "RATING"
  | "SERIES"
  | "STATUS"
  | "SUBTITLE"
  | "TITLE"
  | "%future added value";
export type BooksFilter = {
  author?: ReadonlyArray<string>;
  authorAny?: ReadonlyArray<string>;
  authorNone?: ReadonlyArray<string>;
  coverEmpty?: boolean | null | undefined;
  createdAtGte?: string | null | undefined;
  createdAtLt?: string | null | undefined;
  enrichmentFailed?: boolean | null | undefined;
  genre?: ReadonlyArray<string>;
  genreAny?: ReadonlyArray<string>;
  genreNone?: ReadonlyArray<string>;
  isbn13Contains?: string | null | undefined;
  isbn13Empty?: boolean | null | undefined;
  isbn13Eq?: string | null | undefined;
  mood?: ReadonlyArray<string>;
  moodAny?: ReadonlyArray<string>;
  moodNone?: ReadonlyArray<string>;
  needsReview?: boolean | null | undefined;
  pagesEmpty?: boolean | null | undefined;
  pagesGte?: number | null | undefined;
  pagesLte?: number | null | undefined;
  q?: string | null | undefined;
  ratingEmpty?: boolean | null | undefined;
  ratingGte?: number | null | undefined;
  ratingLte?: number | null | undefined;
  series?: string | null | undefined;
  seriesAny?: ReadonlyArray<string>;
  shelf?: string | null | undefined;
  shelfAny?: ReadonlyArray<string>;
  statusAny?: ReadonlyArray<ReadingStatusFilter>;
  statusNone?: ReadonlyArray<ReadingStatusFilter>;
  subtitleContains?: string | null | undefined;
  subtitleEmpty?: boolean | null | undefined;
  tag?: ReadonlyArray<string>;
  tagAny?: ReadonlyArray<string>;
  tagNone?: ReadonlyArray<string>;
  titleContains?: string | null | undefined;
  titleEq?: string | null | undefined;
  titleNe?: string | null | undefined;
};
export type SortLevel = {
  column: SortField;
  direction: Direction;
};
export type LibraryPageQuery$variables = {
  authorIds: ReadonlyArray<string>;
  count?: number | null | undefined;
  eligible: boolean;
  filter?: BooksFilter | null | undefined;
  seriesIds: ReadonlyArray<string>;
  shelfIds: ReadonlyArray<string>;
  sort?: ReadonlyArray<SortLevel> | null | undefined;
  sourceKey: string;
  summaryEligible: boolean;
  summaryShelfIds: ReadonlyArray<string>;
};
export type LibraryPageQuery$data = {
  readonly libraryFacets?: {
    readonly " $fragmentSpreads": FragmentRefs<"FilterRail_facets" | "LibraryToolbar_facets">;
  };
  readonly librarySummary?: {
    readonly " $fragmentSpreads": FragmentRefs<"LibraryToolbar_summary" | "ShelfHeader_summary">;
  };
  readonly rows?:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"LibraryPage_rows">;
      }
    | null
    | undefined;
  readonly viewer: {
    readonly id: string;
    readonly isChild: boolean;
    readonly libraryPreferences: {
      readonly " $fragmentSpreads": FragmentRefs<"useLibraryPreferences_preferences">;
    };
  };
  readonly " $fragmentSpreads": FragmentRefs<
    "LibraryPage_labels" | "ShelvesListPage_shelves" | "TypeaheadMultiSelect_suggestions"
  >;
};
export type LibraryPageQuery = {
  response: LibraryPageQuery$data;
  variables: LibraryPageQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "authorIds",
    },
    v1 = {
      defaultValue: 50,
      kind: "LocalArgument",
      name: "count",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "eligible",
    },
    v3 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "filter",
    },
    v4 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "seriesIds",
    },
    v5 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "shelfIds",
    },
    v6 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "sort",
    },
    v7 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "sourceKey",
    },
    v8 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "summaryEligible",
    },
    v9 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "summaryShelfIds",
    },
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isChild",
      storageKey: null,
    },
    v12 = [
      {
        kind: "Variable",
        name: "shelfIds",
        variableName: "summaryShelfIds",
      },
    ],
    v13 = {
      kind: "Variable",
      name: "filter",
      variableName: "filter",
    },
    v14 = [v13 /*:: as any*/],
    v15 = {
      kind: "Variable",
      name: "sort",
      variableName: "sort",
    },
    v16 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v17 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v18 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v19 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
    v20 = {
      alias: null,
      args: null,
      concreteType: "LibrarySortLevel",
      kind: "LinkedField",
      name: "sort",
      plural: true,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "column",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "direction",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    v21 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v22 = [v10 /*:: as any*/, v21 /*:: as any*/],
    v23 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "finished",
      storageKey: null,
    },
    v24 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "reading",
      storageKey: null,
    },
    v25 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "unread",
      storageKey: null,
    },
    v26 = [
      v13 /*:: as any*/,
      {
        kind: "Variable",
        name: "first",
        variableName: "count",
      },
      v15 /*:: as any*/,
    ],
    v27 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v28 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v29 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v30 = [v21 /*:: as any*/, v10 /*:: as any*/],
    v31 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v32 = {
      alias: null,
      args: null,
      concreteType: "PageInfo",
      kind: "LinkedField",
      name: "pageInfo",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "endCursor",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "hasNextPage",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    v33 = [
      {
        kind: "Literal",
        name: "first",
        value: 50,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: [
        v0 /*:: as any*/,
        v1 /*:: as any*/,
        v2 /*:: as any*/,
        v3 /*:: as any*/,
        v4 /*:: as any*/,
        v5 /*:: as any*/,
        v6 /*:: as any*/,
        v7 /*:: as any*/,
        v8 /*:: as any*/,
        v9 /*:: as any*/,
      ],
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "LibraryPageQuery",
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "Viewer",
          kind: "LinkedField",
          name: "viewer",
          plural: false,
          selections: [
            v10 /*:: as any*/,
            v11 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "LibraryPreferences",
              kind: "LinkedField",
              name: "libraryPreferences",
              plural: false,
              selections: [
                {
                  args: null,
                  kind: "FragmentSpread",
                  name: "useLibraryPreferences_preferences",
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        {
          args: [
            {
              kind: "Variable",
              name: "authorIds",
              variableName: "authorIds",
            },
            {
              kind: "Variable",
              name: "seriesIds",
              variableName: "seriesIds",
            },
            {
              kind: "Variable",
              name: "shelfIds",
              variableName: "shelfIds",
            },
          ],
          kind: "FragmentSpread",
          name: "LibraryPage_labels",
        },
        {
          condition: "summaryEligible",
          kind: "Condition",
          passingValue: true,
          selections: [
            {
              alias: null,
              args: v12 /*:: as any*/,
              concreteType: "LibrarySummary",
              kind: "LinkedField",
              name: "librarySummary",
              plural: false,
              selections: [
                {
                  args: null,
                  kind: "FragmentSpread",
                  name: "ShelfHeader_summary",
                },
                {
                  args: null,
                  kind: "FragmentSpread",
                  name: "LibraryToolbar_summary",
                },
              ],
              storageKey: null,
            },
          ],
        },
        {
          condition: "eligible",
          kind: "Condition",
          passingValue: true,
          selections: [
            {
              alias: null,
              args: v14 /*:: as any*/,
              concreteType: "LibraryFacets",
              kind: "LinkedField",
              name: "libraryFacets",
              plural: false,
              selections: [
                {
                  args: null,
                  kind: "FragmentSpread",
                  name: "FilterRail_facets",
                },
                {
                  args: null,
                  kind: "FragmentSpread",
                  name: "LibraryToolbar_facets",
                },
              ],
              storageKey: null,
            },
            {
              fragment: {
                kind: "InlineFragment",
                selections: [
                  {
                    args: [
                      {
                        kind: "Variable",
                        name: "count",
                        variableName: "count",
                      },
                      v13 /*:: as any*/,
                      v15 /*:: as any*/,
                      {
                        kind: "Variable",
                        name: "sourceKey",
                        variableName: "sourceKey",
                      },
                    ],
                    kind: "FragmentSpread",
                    name: "LibraryPage_rows",
                  },
                ],
                type: "Query",
                abstractKey: null,
              },
              kind: "AliasedInlineFragmentSpread",
              name: "rows",
            },
          ],
        },
        {
          args: null,
          kind: "FragmentSpread",
          name: "ShelvesListPage_shelves",
        },
        {
          args: null,
          kind: "FragmentSpread",
          name: "TypeaheadMultiSelect_suggestions",
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [
        v0 /*:: as any*/,
        v4 /*:: as any*/,
        v5 /*:: as any*/,
        v2 /*:: as any*/,
        v8 /*:: as any*/,
        v9 /*:: as any*/,
        v7 /*:: as any*/,
        v3 /*:: as any*/,
        v6 /*:: as any*/,
        v1 /*:: as any*/,
      ],
      kind: "Operation",
      name: "LibraryPageQuery",
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "Viewer",
          kind: "LinkedField",
          name: "viewer",
          plural: false,
          selections: [
            v10 /*:: as any*/,
            v11 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "LibraryPreferences",
              kind: "LinkedField",
              name: "libraryPreferences",
              plural: false,
              selections: [
                v16 /*:: as any*/,
                v17 /*:: as any*/,
                v18 /*:: as any*/,
                v19 /*:: as any*/,
                v20 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "LibraryPreferenceDefaults",
                  kind: "LinkedField",
                  name: "defaults",
                  plural: false,
                  selections: [
                    v17 /*:: as any*/,
                    v18 /*:: as any*/,
                    v19 /*:: as any*/,
                    v20 /*:: as any*/,
                  ],
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        {
          alias: null,
          args: [
            {
              kind: "Variable",
              name: "ids",
              variableName: "authorIds",
            },
          ],
          concreteType: "Author",
          kind: "LinkedField",
          name: "authorLabels",
          plural: true,
          selections: v22 /*:: as any*/,
          storageKey: null,
        },
        {
          alias: null,
          args: [
            {
              kind: "Variable",
              name: "ids",
              variableName: "seriesIds",
            },
          ],
          concreteType: "Series",
          kind: "LinkedField",
          name: "seriesLabels",
          plural: true,
          selections: v22 /*:: as any*/,
          storageKey: null,
        },
        {
          alias: null,
          args: [
            {
              kind: "Variable",
              name: "ids",
              variableName: "shelfIds",
            },
          ],
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelfLabels",
          plural: true,
          selections: v22 /*:: as any*/,
          storageKey: null,
        },
        {
          condition: "summaryEligible",
          kind: "Condition",
          passingValue: true,
          selections: [
            {
              alias: null,
              args: v12 /*:: as any*/,
              concreteType: "LibrarySummary",
              kind: "LinkedField",
              name: "librarySummary",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "totalCount",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  concreteType: "ReadingStateCounts",
                  kind: "LinkedField",
                  name: "readingStates",
                  plural: false,
                  selections: [v23 /*:: as any*/, v24 /*:: as any*/, v25 /*:: as any*/],
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  concreteType: "NeedsAttentionCounts",
                  kind: "LinkedField",
                  name: "needsAttention",
                  plural: false,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "toReview",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "enrichmentFailed",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "noIsbn",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "noCover",
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
        },
        {
          condition: "eligible",
          kind: "Condition",
          passingValue: true,
          selections: [
            {
              alias: null,
              args: v14 /*:: as any*/,
              concreteType: "LibraryFacets",
              kind: "LinkedField",
              name: "libraryFacets",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "ReadingStateCounts",
                  kind: "LinkedField",
                  name: "readingStates",
                  plural: false,
                  selections: [
                    v25 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "wantToRead",
                      storageKey: null,
                    },
                    v24 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "onHold",
                      storageKey: null,
                    },
                    v23 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "abandoned",
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  concreteType: "ShelfFacetCount",
                  kind: "LinkedField",
                  name: "shelves",
                  plural: true,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "shelfId",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "count",
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "matchingCount",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: v26 /*:: as any*/,
              concreteType: "ManifestationConnection",
              kind: "LinkedField",
              name: "books",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "ManifestationEdge",
                  kind: "LinkedField",
                  name: "edges",
                  plural: true,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      concreteType: "Manifestation",
                      kind: "LinkedField",
                      name: "node",
                      plural: false,
                      selections: [
                        v10 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          concreteType: "MetadataRevision",
                          kind: "LinkedField",
                          name: "metadata",
                          plural: false,
                          selections: [
                            v27 /*:: as any*/,
                            v10 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "isbn13",
                              storageKey: null,
                            },
                            v28 /*:: as any*/,
                            v29 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "pages",
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: null,
                          concreteType: "ReadingRevision",
                          kind: "LinkedField",
                          name: "readingState",
                          plural: false,
                          selections: [
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "rating",
                              storageKey: null,
                            },
                            v10 /*:: as any*/,
                            v28 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "status",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "progressPct",
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "coverUrl",
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "createdAt",
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: null,
                          concreteType: "Work",
                          kind: "LinkedField",
                          name: "work",
                          plural: false,
                          selections: [
                            v10 /*:: as any*/,
                            v27 /*:: as any*/,
                            v29 /*:: as any*/,
                            {
                              alias: null,
                              args: [
                                {
                                  kind: "Literal",
                                  name: "first",
                                  value: 5,
                                },
                              ],
                              concreteType: "AuthorConnection",
                              kind: "LinkedField",
                              name: "authors",
                              plural: false,
                              selections: [
                                {
                                  alias: null,
                                  args: null,
                                  concreteType: "AuthorEdge",
                                  kind: "LinkedField",
                                  name: "edges",
                                  plural: true,
                                  selections: [
                                    {
                                      alias: null,
                                      args: null,
                                      concreteType: "Author",
                                      kind: "LinkedField",
                                      name: "node",
                                      plural: false,
                                      selections: v30 /*:: as any*/,
                                      storageKey: null,
                                    },
                                  ],
                                  storageKey: null,
                                },
                              ],
                              storageKey: "authors(first:5)",
                            },
                            {
                              alias: null,
                              args: null,
                              concreteType: "SelectedSeries",
                              kind: "LinkedField",
                              name: "selectedSeries",
                              plural: false,
                              selections: [
                                {
                                  alias: null,
                                  args: null,
                                  concreteType: "Series",
                                  kind: "LinkedField",
                                  name: "node",
                                  plural: false,
                                  selections: v30 /*:: as any*/,
                                  storageKey: null,
                                },
                                {
                                  alias: null,
                                  args: null,
                                  kind: "ScalarField",
                                  name: "position",
                                  storageKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                        v16 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    v31 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                v32 /*:: as any*/,
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: v26 /*:: as any*/,
              filters: ["filter", "sort"],
              handle: "connection",
              key: "LibraryPage_rows_books",
              kind: "LinkedHandle",
              name: "books",
              dynamicKey: {
                kind: "Variable",
                name: "__dynamicKey",
                variableName: "sourceKey",
              },
            },
          ],
        },
        {
          alias: null,
          args: v33 /*:: as any*/,
          concreteType: "ShelfConnection",
          kind: "LinkedField",
          name: "shelves",
          plural: false,
          selections: [
            {
              alias: null,
              args: null,
              concreteType: "ShelfEdge",
              kind: "LinkedField",
              name: "edges",
              plural: true,
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "Shelf",
                  kind: "LinkedField",
                  name: "node",
                  plural: false,
                  selections: [
                    v10 /*:: as any*/,
                    v21 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "isSystem",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "itemCount",
                      storageKey: null,
                    },
                    v16 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                v31 /*:: as any*/,
              ],
              storageKey: null,
            },
            v32 /*:: as any*/,
          ],
          storageKey: "shelves(first:50)",
        },
        {
          alias: null,
          args: v33 /*:: as any*/,
          filters: null,
          handle: "connection",
          key: "ShelvesListPage_shelves",
          kind: "LinkedHandle",
          name: "shelves",
        },
      ],
    },
    params: {
      id: "913627a461d124019b070e057a2056bfdd6c1a44298add9b61d7abc6d9b4e06c",
      metadata: {},
      name: "LibraryPageQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "8c2662d4d319daaf18f3006c520a18fe";

export default node;

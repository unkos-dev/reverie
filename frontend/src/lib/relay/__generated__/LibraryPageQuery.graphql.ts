/**
 * @generated SignedSource<<f687abaf949d15cd83354a126c5a5400>>
 * @relayHash e2950238836ba3b5ee3b0a3b0241c61c
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 2834a2b747f999507bede3e9c9e1a7175810cb82a5378f1f8ac2fd2993be03c0

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type Direction = "ASC" | "DESC" | "%future added value";
export type SortField = "AUTHOR" | "CREATED_AT" | "PAGES" | "TITLE" | "%future added value";
export type BooksFilter = {
  author?: ReadonlyArray<string>;
  authorAny?: ReadonlyArray<string>;
  authorNone?: ReadonlyArray<string>;
  createdAtGte?: string | null | undefined;
  createdAtLt?: string | null | undefined;
  genre?: ReadonlyArray<string>;
  genreAny?: ReadonlyArray<string>;
  genreNone?: ReadonlyArray<string>;
  isbn13Contains?: string | null | undefined;
  isbn13Empty?: boolean | null | undefined;
  isbn13Eq?: string | null | undefined;
  mood?: ReadonlyArray<string>;
  moodAny?: ReadonlyArray<string>;
  moodNone?: ReadonlyArray<string>;
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
  statusAny?: ReadonlyArray<string>;
  statusNone?: ReadonlyArray<string>;
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
};
export type LibraryPageQuery$data = {
  readonly rows?:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerLibrary">;
      }
    | null
    | undefined;
  readonly viewer: {
    readonly id: string;
    readonly libraryPreferences: {
      readonly " $fragmentSpreads": FragmentRefs<"useLibraryPreferences_preferences">;
    };
  };
  readonly " $fragmentSpreads": FragmentRefs<
    "LibraryPage_labels" | "TypeaheadMultiSelect_suggestions" | "readerShelves"
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
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v9 = {
      kind: "Variable",
      name: "filter",
      variableName: "filter",
    },
    v10 = {
      kind: "Variable",
      name: "sort",
      variableName: "sort",
    },
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v12 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v13 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v14 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
    v15 = {
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
    v16 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v17 = [v8 /*:: as any*/, v16 /*:: as any*/],
    v18 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "itemCount",
      storageKey: null,
    },
    v19 = [
      {
        kind: "Literal",
        name: "first",
        value: 50,
      },
    ],
    v20 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v21 = {
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
    v22 = [
      v9 /*:: as any*/,
      {
        kind: "Variable",
        name: "first",
        variableName: "count",
      },
      v10 /*:: as any*/,
    ],
    v23 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v24 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v25 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v26 = [v16 /*:: as any*/, v8 /*:: as any*/];
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
            v8 /*:: as any*/,
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
          args: null,
          kind: "FragmentSpread",
          name: "readerShelves",
        },
        {
          args: null,
          kind: "FragmentSpread",
          name: "TypeaheadMultiSelect_suggestions",
        },
        {
          condition: "eligible",
          kind: "Condition",
          passingValue: true,
          selections: [
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
                      v9 /*:: as any*/,
                      v10 /*:: as any*/,
                      {
                        kind: "Variable",
                        name: "sourceKey",
                        variableName: "sourceKey",
                      },
                    ],
                    kind: "FragmentSpread",
                    name: "readerLibrary",
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
            v8 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "LibraryPreferences",
              kind: "LinkedField",
              name: "libraryPreferences",
              plural: false,
              selections: [
                v11 /*:: as any*/,
                v12 /*:: as any*/,
                v13 /*:: as any*/,
                v14 /*:: as any*/,
                v15 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "LibraryPreferenceDefaults",
                  kind: "LinkedField",
                  name: "defaults",
                  plural: false,
                  selections: [
                    v12 /*:: as any*/,
                    v13 /*:: as any*/,
                    v14 /*:: as any*/,
                    v15 /*:: as any*/,
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
          selections: v17 /*:: as any*/,
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
          selections: v17 /*:: as any*/,
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
          selections: [v8 /*:: as any*/, v16 /*:: as any*/, v18 /*:: as any*/],
          storageKey: null,
        },
        {
          alias: null,
          args: v19 /*:: as any*/,
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
                    v8 /*:: as any*/,
                    v16 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "isSystem",
                      storageKey: null,
                    },
                    v18 /*:: as any*/,
                    v11 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                v20 /*:: as any*/,
              ],
              storageKey: null,
            },
            v21 /*:: as any*/,
          ],
          storageKey: "shelves(first:50)",
        },
        {
          alias: null,
          args: v19 /*:: as any*/,
          filters: null,
          handle: "connection",
          key: "readerShelves_shelves",
          kind: "LinkedHandle",
          name: "shelves",
        },
        {
          condition: "eligible",
          kind: "Condition",
          passingValue: true,
          selections: [
            {
              alias: null,
              args: v22 /*:: as any*/,
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
                        v8 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          concreteType: "MetadataRevision",
                          kind: "LinkedField",
                          name: "metadata",
                          plural: false,
                          selections: [
                            v23 /*:: as any*/,
                            v8 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "isbn13",
                              storageKey: null,
                            },
                            v24 /*:: as any*/,
                            v25 /*:: as any*/,
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
                            v8 /*:: as any*/,
                            v24 /*:: as any*/,
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
                            v8 /*:: as any*/,
                            v23 /*:: as any*/,
                            v25 /*:: as any*/,
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
                                      selections: v26 /*:: as any*/,
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
                                  selections: v26 /*:: as any*/,
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
                        v11 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    v20 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                v21 /*:: as any*/,
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: v22 /*:: as any*/,
              filters: ["filter", "sort"],
              handle: "connection",
              key: "readerLibrary_books",
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
      ],
    },
    params: {
      id: "2834a2b747f999507bede3e9c9e1a7175810cb82a5378f1f8ac2fd2993be03c0",
      metadata: {},
      name: "LibraryPageQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "a3316da9f42c04aa2a243b40da91df1c";

export default node;

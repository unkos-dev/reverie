/**
 * @generated SignedSource<<3475a8305afe9622bead3829071bf3aa>>
 * @relayHash 59e9f36c3bbf3ea37052b602ba592fde
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 380281b28e73a5967fc05f2742e6ff46587658dc4467750cc43e62ff603d1fae

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type Direction = "ASC" | "DESC" | "%future added value";
export type SortField = "author" | "created_at" | "pages" | "title" | "%future added value";
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
export type readerLibraryQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  filter?: BooksFilter | null | undefined;
  sort?: ReadonlyArray<SortLevel> | null | undefined;
};
export type readerLibraryQuery$data = {
  readonly " $fragmentSpreads": FragmentRefs<"readerLibrary">;
};
export type readerLibraryQuery = {
  response: readerLibraryQuery$data;
  variables: readerLibraryQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: 50,
        kind: "LocalArgument",
        name: "count",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "cursor",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "filter",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "sort",
      },
    ],
    v1 = {
      kind: "Variable",
      name: "filter",
      variableName: "filter",
    },
    v2 = {
      kind: "Variable",
      name: "sort",
      variableName: "sort",
    },
    v3 = [
      {
        kind: "Variable",
        name: "after",
        variableName: "cursor",
      },
      v1 /*:: as any*/,
      {
        kind: "Variable",
        name: "first",
        variableName: "count",
      },
      v2 /*:: as any*/,
    ],
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hasNextPage",
      storageKey: null,
    },
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "endCursor",
      storageKey: null,
    },
    v12 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerLibraryQuery",
      selections: [
        {
          args: [
            {
              kind: "Variable",
              name: "count",
              variableName: "count",
            },
            {
              kind: "Variable",
              name: "cursor",
              variableName: "cursor",
            },
            v1 /*:: as any*/,
            v2 /*:: as any*/,
          ],
          kind: "FragmentSpread",
          name: "readerLibrary",
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerLibraryQuery",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
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
                    v4 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "coverUrl",
                      storageKey: null,
                    },
                    v5 /*:: as any*/,
                    v6 /*:: as any*/,
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
                        v4 /*:: as any*/,
                        v7 /*:: as any*/,
                        v8 /*:: as any*/,
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
                                  selections: [v9 /*:: as any*/, v4 /*:: as any*/],
                                  storageKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              concreteType: "PageInfo",
                              kind: "LinkedField",
                              name: "pageInfo",
                              plural: false,
                              selections: [v10 /*:: as any*/, v11 /*:: as any*/],
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
                              selections: [v4 /*:: as any*/, v9 /*:: as any*/],
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
                    {
                      alias: null,
                      args: null,
                      concreteType: "MetadataSnapshot",
                      kind: "LinkedField",
                      name: "metadata",
                      plural: false,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "workId",
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "manifestationId",
                          storageKey: null,
                        },
                        v12 /*:: as any*/,
                        v7 /*:: as any*/,
                        v8 /*:: as any*/,
                        v5 /*:: as any*/,
                        v6 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      concreteType: "ReadingState",
                      kind: "LinkedField",
                      name: "readingState",
                      plural: false,
                      selections: [
                        v12 /*:: as any*/,
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
                          name: "rating",
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "__typename",
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "cursor",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              concreteType: "PageInfo",
              kind: "LinkedField",
              name: "pageInfo",
              plural: false,
              selections: [v11 /*:: as any*/, v10 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        {
          alias: null,
          args: v3 /*:: as any*/,
          filters: ["filter", "sort"],
          handle: "connection",
          key: "readerLibrary_books",
          kind: "LinkedHandle",
          name: "books",
        },
      ],
    },
    params: {
      id: "380281b28e73a5967fc05f2742e6ff46587658dc4467750cc43e62ff603d1fae",
      metadata: {},
      name: "readerLibraryQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "e4c60f3ef0ef39eaa688f5414115c1a2";

export default node;

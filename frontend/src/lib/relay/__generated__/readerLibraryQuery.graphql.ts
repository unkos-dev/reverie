/**
 * @generated SignedSource<<7dbf321a3f69a817bed5859054150c92>>
 * @relayHash 93280cd2056ae60a8fad45268329d146
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 242de783e8572bcd63626db7184bae8c65fc0481f1dc0e315907aaa5c44c4c1f

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
export type readerLibraryQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  filter?: BooksFilter | null | undefined;
  sort?: ReadonlyArray<SortLevel> | null | undefined;
  sourceKey: string;
};
export type readerLibraryQuery$data = {
  readonly " $fragmentSpreads": FragmentRefs<"readerLibrary">;
};
export type readerLibraryQuery$rawResponse = {
  readonly books: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly __typename: "Manifestation";
        readonly coverUrl: string;
        readonly createdAt: string;
        readonly id: string;
        readonly isbn13: string | null | undefined;
        readonly metadata: {
          readonly etag: string;
          readonly isbn13: string | null | undefined;
          readonly manifestationId: string;
          readonly pages: number | null | undefined;
          readonly subtitle: string | null | undefined;
          readonly title: string;
          readonly workId: string;
        };
        readonly pages: number | null | undefined;
        readonly readingState: {
          readonly etag: string;
          readonly finishedAt: string | null | undefined;
          readonly progressPct: number | null | undefined;
          readonly rating: number | null | undefined;
          readonly startedAt: string | null | undefined;
          readonly status: string | null | undefined;
        };
        readonly work:
          | {
              readonly authors: {
                readonly edges: ReadonlyArray<{
                  readonly node: {
                    readonly id: string;
                    readonly name: string;
                  };
                }>;
                readonly pageInfo: {
                  readonly endCursor: string | null | undefined;
                  readonly hasNextPage: boolean;
                };
              };
              readonly id: string;
              readonly selectedSeries:
                | {
                    readonly node: {
                      readonly id: string;
                      readonly name: string;
                    };
                    readonly position: number | null | undefined;
                  }
                | null
                | undefined;
              readonly subtitle: string | null | undefined;
              readonly title: string;
            }
          | null
          | undefined;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
    };
  };
};
export type readerLibraryQuery = {
  rawResponse: readerLibraryQuery$rawResponse;
  response: readerLibraryQuery$data;
  variables: readerLibraryQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: 50,
      kind: "LocalArgument",
      name: "count",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "cursor",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "filter",
    },
    v3 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "sort",
    },
    v4 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "sourceKey",
    },
    v5 = {
      kind: "Variable",
      name: "filter",
      variableName: "filter",
    },
    v6 = {
      kind: "Variable",
      name: "sort",
      variableName: "sort",
    },
    v7 = [
      {
        kind: "Variable",
        name: "after",
        variableName: "cursor",
      },
      v5 /*:: as any*/,
      {
        kind: "Variable",
        name: "first",
        variableName: "count",
      },
      v6 /*:: as any*/,
    ],
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v12 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v13 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v14 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hasNextPage",
      storageKey: null,
    },
    v15 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "endCursor",
      storageKey: null,
    },
    v16 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [
        v0 /*:: as any*/,
        v1 /*:: as any*/,
        v2 /*:: as any*/,
        v3 /*:: as any*/,
        v4 /*:: as any*/,
      ],
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
            v5 /*:: as any*/,
            v6 /*:: as any*/,
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
    kind: "Request",
    operation: {
      argumentDefinitions: [
        v0 /*:: as any*/,
        v1 /*:: as any*/,
        v4 /*:: as any*/,
        v2 /*:: as any*/,
        v3 /*:: as any*/,
      ],
      kind: "Operation",
      name: "readerLibraryQuery",
      selections: [
        {
          alias: null,
          args: v7 /*:: as any*/,
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
                      kind: "ScalarField",
                      name: "coverUrl",
                      storageKey: null,
                    },
                    v9 /*:: as any*/,
                    v10 /*:: as any*/,
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
                        v11 /*:: as any*/,
                        v12 /*:: as any*/,
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
                                  selections: [v13 /*:: as any*/, v8 /*:: as any*/],
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
                              selections: [v14 /*:: as any*/, v15 /*:: as any*/],
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
                              selections: [v8 /*:: as any*/, v13 /*:: as any*/],
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
                        v16 /*:: as any*/,
                        v11 /*:: as any*/,
                        v12 /*:: as any*/,
                        v9 /*:: as any*/,
                        v10 /*:: as any*/,
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
                        v16 /*:: as any*/,
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
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "progressPct",
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "startedAt",
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "finishedAt",
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
              selections: [v15 /*:: as any*/, v14 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        {
          alias: null,
          args: v7 /*:: as any*/,
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
    params: {
      id: "242de783e8572bcd63626db7184bae8c65fc0481f1dc0e315907aaa5c44c4c1f",
      metadata: {},
      name: "readerLibraryQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "a157247d3d6b5529c35fec423410909e";

export default node;

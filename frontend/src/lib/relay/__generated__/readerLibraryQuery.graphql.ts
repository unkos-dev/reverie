/**
 * @generated SignedSource<<eda231add118af76a9c22eab3ca3119e>>
 * @relayHash beecfb1aeeaa9d34c5b872864f74c665
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID f0c0eddc5360214ab23679ba46ed3f6ac24012ddf24d804f7a9b214b9a0410a8

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
  after?: string | null | undefined;
  count?: number | null | undefined;
  filter?: BooksFilter | null | undefined;
  sort?: ReadonlyArray<SortLevel> | null | undefined;
};
export type readerLibraryQuery$data = {
  readonly books: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly " $fragmentSpreads": FragmentRefs<"readerBook">;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
};
export type readerLibraryQuery = {
  response: readerLibraryQuery$data;
  variables: readerLibraryQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "after",
    },
    v1 = {
      defaultValue: 50,
      kind: "LocalArgument",
      name: "count",
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
    v4 = [
      {
        kind: "Variable",
        name: "after",
        variableName: "after",
      },
      {
        kind: "Variable",
        name: "filter",
        variableName: "filter",
      },
      {
        kind: "Variable",
        name: "first",
        variableName: "count",
      },
      {
        kind: "Variable",
        name: "sort",
        variableName: "sort",
      },
    ],
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v6 = {
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
          name: "hasNextPage",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "hasPreviousPage",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "startCursor",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "endCursor",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "description",
      storageKey: null,
    },
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "language",
      storageKey: null,
    },
    v12 = [
      v7 /*:: as any*/,
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "name",
        storageKey: null,
      },
    ],
    v13 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "position",
      storageKey: null,
    },
    v14 = [
      {
        kind: "Literal",
        name: "first",
        value: 5,
      },
    ],
    v15 = [
      {
        alias: null,
        args: null,
        concreteType: "AuthorEdge",
        kind: "LinkedField",
        name: "edges",
        plural: true,
        selections: [
          v5 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "role",
            storageKey: null,
          },
          v13 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "Author",
            kind: "LinkedField",
            name: "node",
            plural: false,
            selections: v12 /*:: as any*/,
            storageKey: null,
          },
        ],
        storageKey: null,
      },
      v6 /*:: as any*/,
    ],
    v16 = {
      alias: null,
      args: v14 /*:: as any*/,
      concreteType: "ExternalIdentifierConnection",
      kind: "LinkedField",
      name: "identifiers",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "ExternalIdentifierEdge",
          kind: "LinkedField",
          name: "edges",
          plural: true,
          selections: [
            v5 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "ExternalIdentifier",
              kind: "LinkedField",
              name: "node",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "level",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "scheme",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "value",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        v6 /*:: as any*/,
      ],
      storageKey: "identifiers(first:5)",
    },
    v17 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v18 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
      storageKey: null,
    },
    v19 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v20 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v21 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v22 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "contentRating",
      storageKey: null,
    },
    v23 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v24 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "rating",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/, v3 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "readerLibraryQuery",
      selections: [
        {
          alias: null,
          args: v4 /*:: as any*/,
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
                v5 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Manifestation",
                  kind: "LinkedField",
                  name: "node",
                  plural: false,
                  selections: [
                    {
                      args: null,
                      kind: "FragmentSpread",
                      name: "readerBook",
                    },
                  ],
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
            v6 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v2 /*:: as any*/, v3 /*:: as any*/],
      kind: "Operation",
      name: "readerLibraryQuery",
      selections: [
        {
          alias: null,
          args: v4 /*:: as any*/,
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
                v5 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Manifestation",
                  kind: "LinkedField",
                  name: "node",
                  plural: false,
                  selections: [
                    v7 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "work",
                      plural: false,
                      selections: [
                        v7 /*:: as any*/,
                        v8 /*:: as any*/,
                        v9 /*:: as any*/,
                        v10 /*:: as any*/,
                        v11 /*:: as any*/,
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
                              selections: v12 /*:: as any*/,
                              storageKey: null,
                            },
                            v13 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: v14 /*:: as any*/,
                          concreteType: "AuthorConnection",
                          kind: "LinkedField",
                          name: "authors",
                          plural: false,
                          selections: v15 /*:: as any*/,
                          storageKey: "authors(first:5)",
                        },
                        {
                          alias: null,
                          args: v14 /*:: as any*/,
                          concreteType: "AuthorConnection",
                          kind: "LinkedField",
                          name: "contributors",
                          plural: false,
                          selections: v15 /*:: as any*/,
                          storageKey: "contributors(first:5)",
                        },
                        v16 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    v17 /*:: as any*/,
                    v18 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "isbnMismatch",
                      storageKey: null,
                    },
                    v19 /*:: as any*/,
                    v20 /*:: as any*/,
                    v21 /*:: as any*/,
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
                      kind: "ScalarField",
                      name: "updatedAt",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "coverUrl",
                      storageKey: null,
                    },
                    v22 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "ingestionStatus",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "validationStatus",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "enrichmentStatus",
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
                        v23 /*:: as any*/,
                        v8 /*:: as any*/,
                        v9 /*:: as any*/,
                        v10 /*:: as any*/,
                        v11 /*:: as any*/,
                        v18 /*:: as any*/,
                        v17 /*:: as any*/,
                        v19 /*:: as any*/,
                        v20 /*:: as any*/,
                        v21 /*:: as any*/,
                        v22 /*:: as any*/,
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
                        v23 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "status",
                          storageKey: null,
                        },
                        v24 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "notes",
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
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "lastReadAt",
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: v14 /*:: as any*/,
                      concreteType: "GenreConnection",
                      kind: "LinkedField",
                      name: "genres",
                      plural: false,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          concreteType: "GenreEdge",
                          kind: "LinkedField",
                          name: "edges",
                          plural: true,
                          selections: [
                            v5 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              concreteType: "Genre",
                              kind: "LinkedField",
                              name: "node",
                              plural: false,
                              selections: v12 /*:: as any*/,
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                        v6 /*:: as any*/,
                      ],
                      storageKey: "genres(first:5)",
                    },
                    {
                      alias: null,
                      args: v14 /*:: as any*/,
                      concreteType: "MoodConnection",
                      kind: "LinkedField",
                      name: "moods",
                      plural: false,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          concreteType: "MoodEdge",
                          kind: "LinkedField",
                          name: "edges",
                          plural: true,
                          selections: [
                            v5 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              concreteType: "Mood",
                              kind: "LinkedField",
                              name: "node",
                              plural: false,
                              selections: v12 /*:: as any*/,
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                        v6 /*:: as any*/,
                      ],
                      storageKey: "moods(first:5)",
                    },
                    {
                      alias: null,
                      args: [
                        {
                          kind: "Literal",
                          name: "first",
                          value: 10,
                        },
                      ],
                      concreteType: "TagConnection",
                      kind: "LinkedField",
                      name: "tags",
                      plural: false,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          concreteType: "TagEdge",
                          kind: "LinkedField",
                          name: "edges",
                          plural: true,
                          selections: [
                            v5 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              concreteType: "Tag",
                              kind: "LinkedField",
                              name: "node",
                              plural: false,
                              selections: v12 /*:: as any*/,
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                        v6 /*:: as any*/,
                      ],
                      storageKey: "tags(first:10)",
                    },
                    v16 /*:: as any*/,
                    {
                      alias: null,
                      args: v14 /*:: as any*/,
                      concreteType: "ExternalRatingConnection",
                      kind: "LinkedField",
                      name: "ratings",
                      plural: false,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          concreteType: "ExternalRatingEdge",
                          kind: "LinkedField",
                          name: "edges",
                          plural: true,
                          selections: [
                            v5 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              concreteType: "ExternalRating",
                              kind: "LinkedField",
                              name: "node",
                              plural: false,
                              selections: [
                                {
                                  alias: null,
                                  args: null,
                                  kind: "ScalarField",
                                  name: "source",
                                  storageKey: null,
                                },
                                v24 /*:: as any*/,
                                {
                                  alias: null,
                                  args: null,
                                  kind: "ScalarField",
                                  name: "scale",
                                  storageKey: null,
                                },
                                {
                                  alias: null,
                                  args: null,
                                  kind: "ScalarField",
                                  name: "reviewCount",
                                  storageKey: null,
                                },
                                {
                                  alias: null,
                                  args: null,
                                  kind: "ScalarField",
                                  name: "fetchedAt",
                                  storageKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                        v6 /*:: as any*/,
                      ],
                      storageKey: "ratings(first:5)",
                    },
                  ],
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
            v6 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "f0c0eddc5360214ab23679ba46ed3f6ac24012ddf24d804f7a9b214b9a0410a8",
      metadata: {},
      name: "readerLibraryQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "9e5d05b0c9f2e29b406daa8284ef84b1";

export default node;

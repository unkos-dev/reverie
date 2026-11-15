/**
 * @generated SignedSource<<9f225620582a63df014ea979913cbcd9>>
 * @relayHash 748a414d9c59212d4b06d4b3e623f5dd
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID bd96cde2e1230a036b2580126b241d9c1f2b679a7ddf9390d95afde4c9e65693

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSeriesQuery$variables = {
  after?: string | null | undefined;
  count?: number | null | undefined;
  id: string;
  manifestationCount?: number | null | undefined;
};
export type readerSeriesQuery$data = {
  readonly series:
    | {
        readonly id: string;
        readonly name: string;
        readonly works: {
          readonly edges: ReadonlyArray<{
            readonly cursor: string;
            readonly node: {
              readonly manifestations: {
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
              readonly " $fragmentSpreads": FragmentRefs<"readerWork">;
            };
            readonly position: number | null | undefined;
          }>;
          readonly pageInfo: {
            readonly endCursor: string | null | undefined;
            readonly hasNextPage: boolean;
            readonly hasPreviousPage: boolean;
            readonly startCursor: string | null | undefined;
          };
        };
      }
    | null
    | undefined;
};
export type readerSeriesQuery = {
  response: readerSeriesQuery$data;
  variables: readerSeriesQuery$variables;
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
      name: "id",
    },
    v3 = {
      defaultValue: 2,
      kind: "LocalArgument",
      name: "manifestationCount",
    },
    v4 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
    ],
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v7 = [
      {
        kind: "Variable",
        name: "after",
        variableName: "after",
      },
      {
        kind: "Variable",
        name: "first",
        variableName: "count",
      },
    ],
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "position",
      storageKey: null,
    },
    v10 = [
      {
        kind: "Variable",
        name: "first",
        variableName: "manifestationCount",
      },
    ],
    v11 = {
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
    v12 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v13 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v14 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "description",
      storageKey: null,
    },
    v15 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "language",
      storageKey: null,
    },
    v16 = [v5 /*:: as any*/, v6 /*:: as any*/],
    v17 = {
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
          selections: v16 /*:: as any*/,
          storageKey: null,
        },
        v9 /*:: as any*/,
      ],
      storageKey: null,
    },
    v18 = [
      {
        kind: "Literal",
        name: "first",
        value: 5,
      },
    ],
    v19 = [
      {
        alias: null,
        args: null,
        concreteType: "AuthorEdge",
        kind: "LinkedField",
        name: "edges",
        plural: true,
        selections: [
          v8 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "role",
            storageKey: null,
          },
          v9 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "Author",
            kind: "LinkedField",
            name: "node",
            plural: false,
            selections: v16 /*:: as any*/,
            storageKey: null,
          },
        ],
        storageKey: null,
      },
      v11 /*:: as any*/,
    ],
    v20 = {
      alias: null,
      args: v18 /*:: as any*/,
      concreteType: "AuthorConnection",
      kind: "LinkedField",
      name: "authors",
      plural: false,
      selections: v19 /*:: as any*/,
      storageKey: "authors(first:5)",
    },
    v21 = {
      alias: null,
      args: v18 /*:: as any*/,
      concreteType: "AuthorConnection",
      kind: "LinkedField",
      name: "contributors",
      plural: false,
      selections: v19 /*:: as any*/,
      storageKey: "contributors(first:5)",
    },
    v22 = {
      alias: null,
      args: v18 /*:: as any*/,
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
            v8 /*:: as any*/,
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
        v11 /*:: as any*/,
      ],
      storageKey: "identifiers(first:5)",
    },
    v23 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v24 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
      storageKey: null,
    },
    v25 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v26 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v27 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v28 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "contentRating",
      storageKey: null,
    },
    v29 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v30 = {
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
      name: "readerSeriesQuery",
      selections: [
        {
          alias: null,
          args: v4 /*:: as any*/,
          concreteType: "Series",
          kind: "LinkedField",
          name: "series",
          plural: false,
          selections: [
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            {
              alias: null,
              args: v7 /*:: as any*/,
              concreteType: "WorkConnection",
              kind: "LinkedField",
              name: "works",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "WorkEdge",
                  kind: "LinkedField",
                  name: "edges",
                  plural: true,
                  selections: [
                    v8 /*:: as any*/,
                    v9 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "node",
                      plural: false,
                      selections: [
                        {
                          args: null,
                          kind: "FragmentSpread",
                          name: "readerWork",
                        },
                        {
                          alias: null,
                          args: v10 /*:: as any*/,
                          concreteType: "ManifestationConnection",
                          kind: "LinkedField",
                          name: "manifestations",
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
                                v8 /*:: as any*/,
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
                            v11 /*:: as any*/,
                          ],
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
          ],
          storageKey: null,
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v3 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "readerSeriesQuery",
      selections: [
        {
          alias: null,
          args: v4 /*:: as any*/,
          concreteType: "Series",
          kind: "LinkedField",
          name: "series",
          plural: false,
          selections: [
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            {
              alias: null,
              args: v7 /*:: as any*/,
              concreteType: "WorkConnection",
              kind: "LinkedField",
              name: "works",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "WorkEdge",
                  kind: "LinkedField",
                  name: "edges",
                  plural: true,
                  selections: [
                    v8 /*:: as any*/,
                    v9 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "node",
                      plural: false,
                      selections: [
                        v5 /*:: as any*/,
                        v12 /*:: as any*/,
                        v13 /*:: as any*/,
                        v14 /*:: as any*/,
                        v15 /*:: as any*/,
                        v17 /*:: as any*/,
                        v20 /*:: as any*/,
                        v21 /*:: as any*/,
                        v22 /*:: as any*/,
                        {
                          alias: null,
                          args: v10 /*:: as any*/,
                          concreteType: "ManifestationConnection",
                          kind: "LinkedField",
                          name: "manifestations",
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
                                v8 /*:: as any*/,
                                {
                                  alias: null,
                                  args: null,
                                  concreteType: "Manifestation",
                                  kind: "LinkedField",
                                  name: "node",
                                  plural: false,
                                  selections: [
                                    v5 /*:: as any*/,
                                    {
                                      alias: null,
                                      args: null,
                                      concreteType: "Work",
                                      kind: "LinkedField",
                                      name: "work",
                                      plural: false,
                                      selections: [
                                        v5 /*:: as any*/,
                                        v12 /*:: as any*/,
                                        v13 /*:: as any*/,
                                        v14 /*:: as any*/,
                                        v15 /*:: as any*/,
                                        v17 /*:: as any*/,
                                        v20 /*:: as any*/,
                                        v21 /*:: as any*/,
                                        v22 /*:: as any*/,
                                      ],
                                      storageKey: null,
                                    },
                                    v23 /*:: as any*/,
                                    v24 /*:: as any*/,
                                    {
                                      alias: null,
                                      args: null,
                                      kind: "ScalarField",
                                      name: "isbnMismatch",
                                      storageKey: null,
                                    },
                                    v25 /*:: as any*/,
                                    v26 /*:: as any*/,
                                    v27 /*:: as any*/,
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
                                    v28 /*:: as any*/,
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
                                        v29 /*:: as any*/,
                                        v12 /*:: as any*/,
                                        v13 /*:: as any*/,
                                        v14 /*:: as any*/,
                                        v15 /*:: as any*/,
                                        v24 /*:: as any*/,
                                        v23 /*:: as any*/,
                                        v25 /*:: as any*/,
                                        v26 /*:: as any*/,
                                        v27 /*:: as any*/,
                                        v28 /*:: as any*/,
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
                                        v29 /*:: as any*/,
                                        {
                                          alias: null,
                                          args: null,
                                          kind: "ScalarField",
                                          name: "status",
                                          storageKey: null,
                                        },
                                        v30 /*:: as any*/,
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
                                      args: v18 /*:: as any*/,
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
                                            v8 /*:: as any*/,
                                            {
                                              alias: null,
                                              args: null,
                                              concreteType: "Genre",
                                              kind: "LinkedField",
                                              name: "node",
                                              plural: false,
                                              selections: v16 /*:: as any*/,
                                              storageKey: null,
                                            },
                                          ],
                                          storageKey: null,
                                        },
                                        v11 /*:: as any*/,
                                      ],
                                      storageKey: "genres(first:5)",
                                    },
                                    {
                                      alias: null,
                                      args: v18 /*:: as any*/,
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
                                            v8 /*:: as any*/,
                                            {
                                              alias: null,
                                              args: null,
                                              concreteType: "Mood",
                                              kind: "LinkedField",
                                              name: "node",
                                              plural: false,
                                              selections: v16 /*:: as any*/,
                                              storageKey: null,
                                            },
                                          ],
                                          storageKey: null,
                                        },
                                        v11 /*:: as any*/,
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
                                            v8 /*:: as any*/,
                                            {
                                              alias: null,
                                              args: null,
                                              concreteType: "Tag",
                                              kind: "LinkedField",
                                              name: "node",
                                              plural: false,
                                              selections: v16 /*:: as any*/,
                                              storageKey: null,
                                            },
                                          ],
                                          storageKey: null,
                                        },
                                        v11 /*:: as any*/,
                                      ],
                                      storageKey: "tags(first:10)",
                                    },
                                    v22 /*:: as any*/,
                                    {
                                      alias: null,
                                      args: v18 /*:: as any*/,
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
                                            v8 /*:: as any*/,
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
                                                v30 /*:: as any*/,
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
                                        v11 /*:: as any*/,
                                      ],
                                      storageKey: "ratings(first:5)",
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
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "bd96cde2e1230a036b2580126b241d9c1f2b679a7ddf9390d95afde4c9e65693",
      metadata: {},
      name: "readerSeriesQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "9b26d078483a8aa9150b35627899ad8d";

export default node;

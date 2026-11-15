/**
 * @generated SignedSource<<2c62211a6d44994956b1b5fa103fd574>>
 * @relayHash d164fcb37b4027cf222415c5f5370a0f
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 0521e572d556c26c010a51c55eba423afcd143524175725a312a46e2d98cfdeb

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSeriesQuery$variables = {
  after?: string | null | undefined;
  id: string;
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
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v2 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
    ],
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v5 = [
      {
        kind: "Variable",
        name: "after",
        variableName: "after",
      },
    ],
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "position",
      storageKey: null,
    },
    v8 = {
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
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "description",
      storageKey: null,
    },
    v12 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "language",
      storageKey: null,
    },
    v13 = [v3 /*:: as any*/, v4 /*:: as any*/],
    v14 = {
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
          selections: v13 /*:: as any*/,
          storageKey: null,
        },
        v7 /*:: as any*/,
      ],
      storageKey: null,
    },
    v15 = [
      {
        alias: null,
        args: null,
        concreteType: "AuthorEdge",
        kind: "LinkedField",
        name: "edges",
        plural: true,
        selections: [
          v6 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "role",
            storageKey: null,
          },
          v7 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "Author",
            kind: "LinkedField",
            name: "node",
            plural: false,
            selections: v13 /*:: as any*/,
            storageKey: null,
          },
        ],
        storageKey: null,
      },
      v8 /*:: as any*/,
    ],
    v16 = {
      alias: null,
      args: null,
      concreteType: "AuthorConnection",
      kind: "LinkedField",
      name: "authors",
      plural: false,
      selections: v15 /*:: as any*/,
      storageKey: null,
    },
    v17 = {
      alias: null,
      args: null,
      concreteType: "AuthorConnection",
      kind: "LinkedField",
      name: "contributors",
      plural: false,
      selections: v15 /*:: as any*/,
      storageKey: null,
    },
    v18 = {
      alias: null,
      args: null,
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
            v6 /*:: as any*/,
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
        v8 /*:: as any*/,
      ],
      storageKey: null,
    },
    v19 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v20 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
      storageKey: null,
    },
    v21 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v22 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v23 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v24 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "contentRating",
      storageKey: null,
    },
    v25 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v26 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "rating",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "readerSeriesQuery",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: "Series",
          kind: "LinkedField",
          name: "series",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            v4 /*:: as any*/,
            {
              alias: null,
              args: v5 /*:: as any*/,
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
                    v6 /*:: as any*/,
                    v7 /*:: as any*/,
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
                          args: null,
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
                                v6 /*:: as any*/,
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
                            v8 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
                v8 /*:: as any*/,
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
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "readerSeriesQuery",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: "Series",
          kind: "LinkedField",
          name: "series",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            v4 /*:: as any*/,
            {
              alias: null,
              args: v5 /*:: as any*/,
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
                    v6 /*:: as any*/,
                    v7 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "node",
                      plural: false,
                      selections: [
                        v3 /*:: as any*/,
                        v9 /*:: as any*/,
                        v10 /*:: as any*/,
                        v11 /*:: as any*/,
                        v12 /*:: as any*/,
                        v14 /*:: as any*/,
                        v16 /*:: as any*/,
                        v17 /*:: as any*/,
                        v18 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
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
                                v6 /*:: as any*/,
                                {
                                  alias: null,
                                  args: null,
                                  concreteType: "Manifestation",
                                  kind: "LinkedField",
                                  name: "node",
                                  plural: false,
                                  selections: [
                                    v3 /*:: as any*/,
                                    {
                                      alias: null,
                                      args: null,
                                      concreteType: "Work",
                                      kind: "LinkedField",
                                      name: "work",
                                      plural: false,
                                      selections: [
                                        v3 /*:: as any*/,
                                        v9 /*:: as any*/,
                                        v10 /*:: as any*/,
                                        v11 /*:: as any*/,
                                        v12 /*:: as any*/,
                                        v14 /*:: as any*/,
                                        v16 /*:: as any*/,
                                        v17 /*:: as any*/,
                                        v18 /*:: as any*/,
                                      ],
                                      storageKey: null,
                                    },
                                    v19 /*:: as any*/,
                                    v20 /*:: as any*/,
                                    {
                                      alias: null,
                                      args: null,
                                      kind: "ScalarField",
                                      name: "isbnMismatch",
                                      storageKey: null,
                                    },
                                    v21 /*:: as any*/,
                                    v22 /*:: as any*/,
                                    v23 /*:: as any*/,
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
                                    v24 /*:: as any*/,
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
                                        v25 /*:: as any*/,
                                        v9 /*:: as any*/,
                                        v10 /*:: as any*/,
                                        v11 /*:: as any*/,
                                        v12 /*:: as any*/,
                                        v20 /*:: as any*/,
                                        v19 /*:: as any*/,
                                        v21 /*:: as any*/,
                                        v22 /*:: as any*/,
                                        v23 /*:: as any*/,
                                        v24 /*:: as any*/,
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
                                        v25 /*:: as any*/,
                                        {
                                          alias: null,
                                          args: null,
                                          kind: "ScalarField",
                                          name: "status",
                                          storageKey: null,
                                        },
                                        v26 /*:: as any*/,
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
                                      args: null,
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
                                            v6 /*:: as any*/,
                                            {
                                              alias: null,
                                              args: null,
                                              concreteType: "Genre",
                                              kind: "LinkedField",
                                              name: "node",
                                              plural: false,
                                              selections: v13 /*:: as any*/,
                                              storageKey: null,
                                            },
                                          ],
                                          storageKey: null,
                                        },
                                        v8 /*:: as any*/,
                                      ],
                                      storageKey: null,
                                    },
                                    {
                                      alias: null,
                                      args: null,
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
                                            v6 /*:: as any*/,
                                            {
                                              alias: null,
                                              args: null,
                                              concreteType: "Mood",
                                              kind: "LinkedField",
                                              name: "node",
                                              plural: false,
                                              selections: v13 /*:: as any*/,
                                              storageKey: null,
                                            },
                                          ],
                                          storageKey: null,
                                        },
                                        v8 /*:: as any*/,
                                      ],
                                      storageKey: null,
                                    },
                                    {
                                      alias: null,
                                      args: null,
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
                                            v6 /*:: as any*/,
                                            {
                                              alias: null,
                                              args: null,
                                              concreteType: "Tag",
                                              kind: "LinkedField",
                                              name: "node",
                                              plural: false,
                                              selections: v13 /*:: as any*/,
                                              storageKey: null,
                                            },
                                          ],
                                          storageKey: null,
                                        },
                                        v8 /*:: as any*/,
                                      ],
                                      storageKey: null,
                                    },
                                    v18 /*:: as any*/,
                                    {
                                      alias: null,
                                      args: null,
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
                                            v6 /*:: as any*/,
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
                                                v26 /*:: as any*/,
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
                                        v8 /*:: as any*/,
                                      ],
                                      storageKey: null,
                                    },
                                  ],
                                  storageKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                            v8 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
                v8 /*:: as any*/,
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "0521e572d556c26c010a51c55eba423afcd143524175725a312a46e2d98cfdeb",
      metadata: {},
      name: "readerSeriesQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "caac081be33b449c6c6335a07214cc92";

export default node;

/**
 * @generated SignedSource<<cf589688faafcd0ac693df259afb1671>>
 * @relayHash dfcee93d120aa142b59d21003c85ea60
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID ba4e8cf9aa09bb1391f6a6cbe992153bda975edf0489f6a5e1e644ac1e70e662

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelfQuery$variables = {
  after?: string | null | undefined;
  id: string;
};
export type readerShelfQuery$data = {
  readonly shelf:
    | {
        readonly createdAt: string;
        readonly id: string;
        readonly isSystem: boolean;
        readonly itemCount: number;
        readonly items: {
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
        readonly name: string;
        readonly updatedAt: string;
      }
    | null
    | undefined;
};
export type readerShelfQuery = {
  response: readerShelfQuery$data;
  variables: readerShelfQuery$variables;
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
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isSystem",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "itemCount",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "createdAt",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "updatedAt",
      storageKey: null,
    },
    v9 = [
      {
        kind: "Variable",
        name: "after",
        variableName: "after",
      },
    ],
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
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
    v16 = [v3 /*:: as any*/, v4 /*:: as any*/],
    v17 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "position",
      storageKey: null,
    },
    v18 = [
      {
        alias: null,
        args: null,
        concreteType: "AuthorEdge",
        kind: "LinkedField",
        name: "edges",
        plural: true,
        selections: [
          v10 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "role",
            storageKey: null,
          },
          v17 /*:: as any*/,
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
    v19 = {
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
            v10 /*:: as any*/,
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
      storageKey: null,
    },
    v20 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v21 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
      storageKey: null,
    },
    v22 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v23 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v24 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v25 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "contentRating",
      storageKey: null,
    },
    v26 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v27 = {
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
      name: "readerShelfQuery",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelf",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            {
              alias: null,
              args: v9 /*:: as any*/,
              concreteType: "ManifestationConnection",
              kind: "LinkedField",
              name: "items",
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
                    v10 /*:: as any*/,
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
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "readerShelfQuery",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelf",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            {
              alias: null,
              args: v9 /*:: as any*/,
              concreteType: "ManifestationConnection",
              kind: "LinkedField",
              name: "items",
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
                    v10 /*:: as any*/,
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
                            v12 /*:: as any*/,
                            v13 /*:: as any*/,
                            v14 /*:: as any*/,
                            v15 /*:: as any*/,
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
                                  selections: v16 /*:: as any*/,
                                  storageKey: null,
                                },
                                v17 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              concreteType: "AuthorConnection",
                              kind: "LinkedField",
                              name: "authors",
                              plural: false,
                              selections: v18 /*:: as any*/,
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              concreteType: "AuthorConnection",
                              kind: "LinkedField",
                              name: "contributors",
                              plural: false,
                              selections: v18 /*:: as any*/,
                              storageKey: null,
                            },
                            v19 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                        v20 /*:: as any*/,
                        v21 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "isbnMismatch",
                          storageKey: null,
                        },
                        v22 /*:: as any*/,
                        v23 /*:: as any*/,
                        v24 /*:: as any*/,
                        v7 /*:: as any*/,
                        v8 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "coverUrl",
                          storageKey: null,
                        },
                        v25 /*:: as any*/,
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
                            v26 /*:: as any*/,
                            v12 /*:: as any*/,
                            v13 /*:: as any*/,
                            v14 /*:: as any*/,
                            v15 /*:: as any*/,
                            v21 /*:: as any*/,
                            v20 /*:: as any*/,
                            v22 /*:: as any*/,
                            v23 /*:: as any*/,
                            v24 /*:: as any*/,
                            v25 /*:: as any*/,
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
                            v26 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "status",
                              storageKey: null,
                            },
                            v27 /*:: as any*/,
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
                                v10 /*:: as any*/,
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
                                v10 /*:: as any*/,
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
                                v10 /*:: as any*/,
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
                          storageKey: null,
                        },
                        v19 /*:: as any*/,
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
                                v10 /*:: as any*/,
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
                                    v27 /*:: as any*/,
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
      id: "ba4e8cf9aa09bb1391f6a6cbe992153bda975edf0489f6a5e1e644ac1e70e662",
      metadata: {},
      name: "readerShelfQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "00db40611717cc3f629b836127a7e2a5";

export default node;

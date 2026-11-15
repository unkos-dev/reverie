/**
 * @generated SignedSource<<5b5c24c601070810ff76daa3ec89a614>>
 * @relayHash d63cdfcf179d1abdfae95f90c0e67beb
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID f1cbbff5001dac787d7cf5a2ca7dd332141ecdaa65c4c25804d3f5a0f10b50e8

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelfQuery$variables = {
  after?: string | null | undefined;
  count?: number | null | undefined;
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
      defaultValue: 50,
      kind: "LocalArgument",
      name: "count",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v3 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
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
      name: "name",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isSystem",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "itemCount",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "createdAt",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "updatedAt",
      storageKey: null,
    },
    v10 = [
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
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v12 = {
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
    v13 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v14 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v15 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "description",
      storageKey: null,
    },
    v16 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "language",
      storageKey: null,
    },
    v17 = [v4 /*:: as any*/, v5 /*:: as any*/],
    v18 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "position",
      storageKey: null,
    },
    v19 = [
      {
        kind: "Literal",
        name: "first",
        value: 5,
      },
    ],
    v20 = [
      {
        alias: null,
        args: null,
        concreteType: "AuthorEdge",
        kind: "LinkedField",
        name: "edges",
        plural: true,
        selections: [
          v11 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "role",
            storageKey: null,
          },
          v18 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "Author",
            kind: "LinkedField",
            name: "node",
            plural: false,
            selections: v17 /*:: as any*/,
            storageKey: null,
          },
        ],
        storageKey: null,
      },
      v12 /*:: as any*/,
    ],
    v21 = {
      alias: null,
      args: v19 /*:: as any*/,
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
            v11 /*:: as any*/,
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
        v12 /*:: as any*/,
      ],
      storageKey: "identifiers(first:5)",
    },
    v22 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v23 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
      storageKey: null,
    },
    v24 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v25 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v26 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v27 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "contentRating",
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
      name: "rating",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "readerShelfQuery",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelf",
          plural: false,
          selections: [
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
            {
              alias: null,
              args: v10 /*:: as any*/,
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
                    v11 /*:: as any*/,
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
                v12 /*:: as any*/,
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
      argumentDefinitions: [v1 /*:: as any*/, v2 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "readerShelfQuery",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelf",
          plural: false,
          selections: [
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
            {
              alias: null,
              args: v10 /*:: as any*/,
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
                    v11 /*:: as any*/,
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
                          concreteType: "Work",
                          kind: "LinkedField",
                          name: "work",
                          plural: false,
                          selections: [
                            v4 /*:: as any*/,
                            v13 /*:: as any*/,
                            v14 /*:: as any*/,
                            v15 /*:: as any*/,
                            v16 /*:: as any*/,
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
                                  selections: v17 /*:: as any*/,
                                  storageKey: null,
                                },
                                v18 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: v19 /*:: as any*/,
                              concreteType: "AuthorConnection",
                              kind: "LinkedField",
                              name: "authors",
                              plural: false,
                              selections: v20 /*:: as any*/,
                              storageKey: "authors(first:5)",
                            },
                            {
                              alias: null,
                              args: v19 /*:: as any*/,
                              concreteType: "AuthorConnection",
                              kind: "LinkedField",
                              name: "contributors",
                              plural: false,
                              selections: v20 /*:: as any*/,
                              storageKey: "contributors(first:5)",
                            },
                            v21 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                        v22 /*:: as any*/,
                        v23 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "isbnMismatch",
                          storageKey: null,
                        },
                        v24 /*:: as any*/,
                        v25 /*:: as any*/,
                        v26 /*:: as any*/,
                        v8 /*:: as any*/,
                        v9 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "coverUrl",
                          storageKey: null,
                        },
                        v27 /*:: as any*/,
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
                            v28 /*:: as any*/,
                            v13 /*:: as any*/,
                            v14 /*:: as any*/,
                            v15 /*:: as any*/,
                            v16 /*:: as any*/,
                            v23 /*:: as any*/,
                            v22 /*:: as any*/,
                            v24 /*:: as any*/,
                            v25 /*:: as any*/,
                            v26 /*:: as any*/,
                            v27 /*:: as any*/,
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
                            v28 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "status",
                              storageKey: null,
                            },
                            v29 /*:: as any*/,
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
                          args: v19 /*:: as any*/,
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
                                v11 /*:: as any*/,
                                {
                                  alias: null,
                                  args: null,
                                  concreteType: "Genre",
                                  kind: "LinkedField",
                                  name: "node",
                                  plural: false,
                                  selections: v17 /*:: as any*/,
                                  storageKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                            v12 /*:: as any*/,
                          ],
                          storageKey: "genres(first:5)",
                        },
                        {
                          alias: null,
                          args: v19 /*:: as any*/,
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
                                v11 /*:: as any*/,
                                {
                                  alias: null,
                                  args: null,
                                  concreteType: "Mood",
                                  kind: "LinkedField",
                                  name: "node",
                                  plural: false,
                                  selections: v17 /*:: as any*/,
                                  storageKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                            v12 /*:: as any*/,
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
                                v11 /*:: as any*/,
                                {
                                  alias: null,
                                  args: null,
                                  concreteType: "Tag",
                                  kind: "LinkedField",
                                  name: "node",
                                  plural: false,
                                  selections: v17 /*:: as any*/,
                                  storageKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                            v12 /*:: as any*/,
                          ],
                          storageKey: "tags(first:10)",
                        },
                        v21 /*:: as any*/,
                        {
                          alias: null,
                          args: v19 /*:: as any*/,
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
                                v11 /*:: as any*/,
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
                                    v29 /*:: as any*/,
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
                            v12 /*:: as any*/,
                          ],
                          storageKey: "ratings(first:5)",
                        },
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
                v12 /*:: as any*/,
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "f1cbbff5001dac787d7cf5a2ca7dd332141ecdaa65c4c25804d3f5a0f10b50e8",
      metadata: {},
      name: "readerShelfQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "abe3881d4cef48935bd4eb3a4d57436f";

export default node;

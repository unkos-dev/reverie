/**
 * @generated SignedSource<<786b88980fbeed331bba85fdc47cc396>>
 * @relayHash 444a06785bc32ff297370a315b0844e9
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 3970a01d82ff13f7ce380de05032a2bea5e5504e7eb58171a1c9fad4ff38c476

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerNodeQuery$variables = {
  id: string;
};
export type readerNodeQuery$data = {
  readonly node:
    | {
        readonly createdAt?: string;
        readonly displayName?: string;
        readonly id: string;
        readonly isChild?: boolean;
        readonly isSystem?: boolean;
        readonly itemCount?: number;
        readonly manifestation?:
          | {
              readonly " $fragmentSpreads": FragmentRefs<"readerBook">;
            }
          | null
          | undefined;
        readonly name?: string;
        readonly updatedAt?: string;
        readonly work?:
          | {
              readonly " $fragmentSpreads": FragmentRefs<"readerWork">;
            }
          | null
          | undefined;
      }
    | null
    | undefined;
};
export type readerNodeQuery = {
  response: readerNodeQuery$data;
  variables: readerNodeQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "id",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "createdAt",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "updatedAt",
      storageKey: null,
    },
    v6 = {
      kind: "InlineFragment",
      selections: [
        v3 /*:: as any*/,
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
        v4 /*:: as any*/,
        v5 /*:: as any*/,
      ],
      type: "Shelf",
      abstractKey: null,
    },
    v7 = {
      kind: "InlineFragment",
      selections: [v3 /*:: as any*/],
      type: "Series",
      abstractKey: null,
    },
    v8 = {
      kind: "InlineFragment",
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "displayName",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "isChild",
          storageKey: null,
        },
      ],
      type: "Viewer",
      abstractKey: null,
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
    v13 = [v2 /*:: as any*/, v3 /*:: as any*/],
    v14 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "position",
      storageKey: null,
    },
    v15 = {
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
        v14 /*:: as any*/,
      ],
      storageKey: null,
    },
    v16 = [
      {
        kind: "Literal",
        name: "first",
        value: 5,
      },
    ],
    v17 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v18 = {
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
    v19 = [
      {
        alias: null,
        args: null,
        concreteType: "AuthorEdge",
        kind: "LinkedField",
        name: "edges",
        plural: true,
        selections: [
          v17 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "role",
            storageKey: null,
          },
          v14 /*:: as any*/,
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
      v18 /*:: as any*/,
    ],
    v20 = {
      alias: null,
      args: v16 /*:: as any*/,
      concreteType: "AuthorConnection",
      kind: "LinkedField",
      name: "authors",
      plural: false,
      selections: v19 /*:: as any*/,
      storageKey: "authors(first:5)",
    },
    v21 = {
      alias: null,
      args: v16 /*:: as any*/,
      concreteType: "AuthorConnection",
      kind: "LinkedField",
      name: "contributors",
      plural: false,
      selections: v19 /*:: as any*/,
      storageKey: "contributors(first:5)",
    },
    v22 = {
      alias: null,
      args: v16 /*:: as any*/,
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
            v17 /*:: as any*/,
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
        v18 /*:: as any*/,
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
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerNodeQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "node",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                {
                  fragment: {
                    kind: "InlineFragment",
                    selections: [
                      {
                        args: null,
                        kind: "FragmentSpread",
                        name: "readerBook",
                      },
                    ],
                    type: "Manifestation",
                    abstractKey: null,
                  },
                  kind: "AliasedInlineFragmentSpread",
                  name: "manifestation",
                },
              ],
              type: "Manifestation",
              abstractKey: null,
            },
            {
              kind: "InlineFragment",
              selections: [
                {
                  fragment: {
                    kind: "InlineFragment",
                    selections: [
                      {
                        args: null,
                        kind: "FragmentSpread",
                        name: "readerWork",
                      },
                    ],
                    type: "Work",
                    abstractKey: null,
                  },
                  kind: "AliasedInlineFragmentSpread",
                  name: "work",
                },
              ],
              type: "Work",
              abstractKey: null,
            },
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerNodeQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "node",
          plural: false,
          selections: [
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "__typename",
              storageKey: null,
            },
            v2 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "Work",
                  kind: "LinkedField",
                  name: "work",
                  plural: false,
                  selections: [
                    v2 /*:: as any*/,
                    v9 /*:: as any*/,
                    v10 /*:: as any*/,
                    v11 /*:: as any*/,
                    v12 /*:: as any*/,
                    v15 /*:: as any*/,
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
                v4 /*:: as any*/,
                v5 /*:: as any*/,
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
                    v9 /*:: as any*/,
                    v10 /*:: as any*/,
                    v11 /*:: as any*/,
                    v12 /*:: as any*/,
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
                  args: v16 /*:: as any*/,
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
                        v17 /*:: as any*/,
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
                    v18 /*:: as any*/,
                  ],
                  storageKey: "genres(first:5)",
                },
                {
                  alias: null,
                  args: v16 /*:: as any*/,
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
                        v17 /*:: as any*/,
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
                    v18 /*:: as any*/,
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
                        v17 /*:: as any*/,
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
                    v18 /*:: as any*/,
                  ],
                  storageKey: "tags(first:10)",
                },
                v22 /*:: as any*/,
                {
                  alias: null,
                  args: v16 /*:: as any*/,
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
                        v17 /*:: as any*/,
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
                    v18 /*:: as any*/,
                  ],
                  storageKey: "ratings(first:5)",
                },
              ],
              type: "Manifestation",
              abstractKey: null,
            },
            {
              kind: "InlineFragment",
              selections: [
                v9 /*:: as any*/,
                v10 /*:: as any*/,
                v11 /*:: as any*/,
                v12 /*:: as any*/,
                v15 /*:: as any*/,
                v20 /*:: as any*/,
                v21 /*:: as any*/,
                v22 /*:: as any*/,
              ],
              type: "Work",
              abstractKey: null,
            },
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "3970a01d82ff13f7ce380de05032a2bea5e5504e7eb58171a1c9fad4ff38c476",
      metadata: {},
      name: "readerNodeQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "93171783b6ee87d42f91078078f55401";

export default node;

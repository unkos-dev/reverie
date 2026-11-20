/**
 * @generated SignedSource<<1f684b68593011292cac25cdb003b5ee>>
 * @relayHash 1c540c74e7646e643ff67d1b8053fe96
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 9138f6ad1cbd25389d9da5807e74130d2c5191bcd9d303d3f0b1b396102466d9

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerDetailQuery$variables = {
  id: string;
};
export type readerDetailQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly work:
          | {
              readonly title: string;
            }
          | null
          | undefined;
        readonly " $fragmentSpreads": FragmentRefs<"readerDetailBook">;
      }
    | null
    | undefined;
};
export type readerDetailQuery = {
  response: readerDetailQuery$data;
  variables: readerDetailQuery$variables;
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
      name: "title",
      storageKey: null,
    },
    v4 = [
      {
        kind: "Literal",
        name: "first",
        value: 20,
      },
    ],
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v6 = [
      v2 /*:: as any*/,
      v5 /*:: as any*/,
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "__typename",
        storageKey: null,
      },
    ],
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
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
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "readerDetailQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "Work",
              kind: "LinkedField",
              name: "work",
              plural: false,
              selections: [v3 /*:: as any*/],
              storageKey: null,
            },
            {
              args: null,
              kind: "FragmentSpread",
              name: "readerDetailBook",
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
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerDetailQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "Work",
              kind: "LinkedField",
              name: "work",
              plural: false,
              selections: [
                v3 /*:: as any*/,
                v2 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "description",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "language",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: v4 /*:: as any*/,
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
                          selections: v6 /*:: as any*/,
                          storageKey: null,
                        },
                        v7 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    v8 /*:: as any*/,
                  ],
                  storageKey: "authors(first:20)",
                },
                {
                  alias: null,
                  args: v4 /*:: as any*/,
                  filters: null,
                  handle: "connection",
                  key: "readerWorkAuthors_authors",
                  kind: "LinkedHandle",
                  name: "authors",
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
                      selections: [v2 /*:: as any*/, v5 /*:: as any*/],
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
              kind: "ScalarField",
              name: "isbn13",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "isbn10",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "isbnMismatch",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "publisher",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "publicationDate",
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
              kind: "ScalarField",
              name: "createdAt",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              concreteType: "MetadataVersions",
              kind: "LinkedField",
              name: "versions",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "pendingCount",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: v4 /*:: as any*/,
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
                    {
                      alias: null,
                      args: null,
                      concreteType: "Tag",
                      kind: "LinkedField",
                      name: "node",
                      plural: false,
                      selections: v6 /*:: as any*/,
                      storageKey: null,
                    },
                    v7 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                v8 /*:: as any*/,
              ],
              storageKey: "tags(first:20)",
            },
            {
              alias: null,
              args: v4 /*:: as any*/,
              filters: null,
              handle: "connection",
              key: "readerDetailTags_tags",
              kind: "LinkedHandle",
              name: "tags",
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "9138f6ad1cbd25389d9da5807e74130d2c5191bcd9d303d3f0b1b396102466d9",
      metadata: {},
      name: "readerDetailQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "6d1e8f89ccda29a0e0833099049dc6f6";

export default node;

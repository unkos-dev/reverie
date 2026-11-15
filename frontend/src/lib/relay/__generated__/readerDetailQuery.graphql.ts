/**
 * @generated SignedSource<<b8ab1e3908bac8513b021fb687deb6c6>>
 * @relayHash ac72fcda159ac7273ab7891c34f2a459
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID c0503fd814c072e187c0cebfe282e29d9d2e4f548f3c4704fc8410261a867a9c

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerDetailQuery$variables = {
  id: string;
};
export type readerDetailQuery$data = {
  readonly manifestation:
    | {
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
      name: "name",
      storageKey: null,
    },
    v4 = [v3 /*:: as any*/, v2 /*:: as any*/],
    v5 = {
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
          name: "endCursor",
          storageKey: null,
        },
      ],
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
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
              kind: "ScalarField",
              name: "coverUrl",
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
              concreteType: "Work",
              kind: "LinkedField",
              name: "work",
              plural: false,
              selections: [
                v2 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "title",
                  storageKey: null,
                },
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
                          selections: v4 /*:: as any*/,
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                    v5 /*:: as any*/,
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
                      selections: [v2 /*:: as any*/, v3 /*:: as any*/],
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
                    {
                      alias: null,
                      args: null,
                      concreteType: "Tag",
                      kind: "LinkedField",
                      name: "node",
                      plural: false,
                      selections: v4 /*:: as any*/,
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
                v5 /*:: as any*/,
              ],
              storageKey: "tags(first:10)",
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "c0503fd814c072e187c0cebfe282e29d9d2e4f548f3c4704fc8410261a867a9c",
      metadata: {},
      name: "readerDetailQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "a09543e65e50fbf5ad831b4734717caf";

export default node;

/**
 * @generated SignedSource<<19ef6d7040768155c81fefb14d5d6333>>
 * @relayHash 1117264e1d68ede0dabdcca909bb8142
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID dfb21f6df2a8948c81f2adee911e0292e199247665b6a8c0fba4b8e231f2f803

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type SeriesPageAuthorsPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type SeriesPageAuthorsPaginationQuery$data = {
  readonly fetch__Work:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"SeriesPage_authors">;
      }
    | null
    | undefined;
};
export type SeriesPageAuthorsPaginationQuery = {
  response: SeriesPageAuthorsPaginationQuery$data;
  variables: SeriesPageAuthorsPaginationQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: 20,
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
    v3 = [
      {
        kind: "Variable",
        name: "after",
        variableName: "cursor",
      },
      {
        kind: "Variable",
        name: "first",
        variableName: "count",
      },
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "SeriesPageAuthorsPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Work",
          kind: "LinkedField",
          name: "fetch__Work",
          plural: false,
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
              ],
              kind: "FragmentSpread",
              name: "SeriesPage_authors",
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
      name: "SeriesPageAuthorsPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Work",
          kind: "LinkedField",
          name: "fetch__Work",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: v3 /*:: as any*/,
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
                      selections: [
                        v2 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "name",
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
                },
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: v3 /*:: as any*/,
              filters: null,
              handle: "connection",
              key: "SeriesPage_authors",
              kind: "LinkedHandle",
              name: "authors",
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "dfb21f6df2a8948c81f2adee911e0292e199247665b6a8c0fba4b8e231f2f803",
      metadata: {},
      name: "SeriesPageAuthorsPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "6523ad2db7f24e513fc2f63421284e4c";

export default node;

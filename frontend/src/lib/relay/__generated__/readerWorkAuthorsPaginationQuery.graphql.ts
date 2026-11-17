/**
 * @generated SignedSource<<8590449dfb726c86749795ed18641153>>
 * @relayHash ec44d9b1ee04f1dd8adb7d9f3fc121b0
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 3036c91388ab1b1bb8a0eeda9bd98621ea285dc28c97eb842525f72f2c0f4fff

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerWorkAuthorsPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type readerWorkAuthorsPaginationQuery$data = {
  readonly fetch__Work:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerWorkAuthors">;
      }
    | null
    | undefined;
};
export type readerWorkAuthorsPaginationQuery = {
  response: readerWorkAuthorsPaginationQuery$data;
  variables: readerWorkAuthorsPaginationQuery$variables;
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
      name: "readerWorkAuthorsPaginationQuery",
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
              name: "readerWorkAuthors",
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
      name: "readerWorkAuthorsPaginationQuery",
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
              key: "readerWorkAuthors_authors",
              kind: "LinkedHandle",
              name: "authors",
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "3036c91388ab1b1bb8a0eeda9bd98621ea285dc28c97eb842525f72f2c0f4fff",
      metadata: {},
      name: "readerWorkAuthorsPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "cb6326f621995644010373b8aa37110d";

export default node;

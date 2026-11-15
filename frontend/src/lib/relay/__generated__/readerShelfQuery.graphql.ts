/**
 * @generated SignedSource<<74c8052833fdb6d091d5068077fcf512>>
 * @relayHash 725ca89a1953a8da7191bcd372afa9d5
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID e9778ee1ef1f0584eae94a9c39e360850a2511ac722610e16283deb2e20b94a7

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelfQuery$variables = {
  id: string;
};
export type readerShelfQuery$data = {
  readonly shelf:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerShelf">;
      }
    | null
    | undefined;
};
export type readerShelfQuery = {
  response: readerShelfQuery$data;
  variables: readerShelfQuery$variables;
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
    v3 = [
      {
        kind: "Literal",
        name: "first",
        value: 50,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerShelfQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelf",
          plural: false,
          selections: [
            {
              args: null,
              kind: "FragmentSpread",
              name: "readerShelf",
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
      name: "readerShelfQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelf",
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
              args: v3 /*:: as any*/,
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
                    {
                      alias: null,
                      args: null,
                      concreteType: "Manifestation",
                      kind: "LinkedField",
                      name: "node",
                      plural: false,
                      selections: [
                        v2 /*:: as any*/,
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
              storageKey: "items(first:50)",
            },
            {
              alias: null,
              args: v3 /*:: as any*/,
              filters: null,
              handle: "connection",
              key: "readerShelfItems_items",
              kind: "LinkedHandle",
              name: "items",
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "e9778ee1ef1f0584eae94a9c39e360850a2511ac722610e16283deb2e20b94a7",
      metadata: {},
      name: "readerShelfQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "333d82aab38dc6e6c833ba41125cf630";

export default node;

/**
 * @generated SignedSource<<44fc52f9c9f59aca83aba843220584f5>>
 * @relayHash bfb32d7e91d0a2828fb8a341cc84359a
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID c874ff3af7a11366ecaf8345c856be6cc4ea5f867498a45b4a3ab7a0547e49a8

import { ConcreteRequest } from "relay-runtime";
export type AddShelfItemInput = {
  manifestationId: string;
  shelfId: string;
};
export type mutationsAddShelfItemMutation$variables = {
  connections: ReadonlyArray<string>;
  input: AddShelfItemInput;
};
export type mutationsAddShelfItemMutation$data = {
  readonly addShelfItem:
    | {
        readonly __typename: "NotFound";
        readonly message: string;
      }
    | {
        readonly __typename: "ShelfItemAdded";
        readonly edge: {
          readonly cursor: string;
          readonly node: {
            readonly id: string;
          };
        };
        readonly shelf: {
          readonly id: string;
          readonly itemCount: number;
          readonly updatedAt: string;
        };
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type mutationsAddShelfItemMutation = {
  response: mutationsAddShelfItemMutation$data;
  variables: mutationsAddShelfItemMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "connections",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "input",
    },
    v2 = [
      {
        kind: "Variable",
        name: "input",
        variableName: "input",
      },
    ],
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
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
      concreteType: "Shelf",
      kind: "LinkedField",
      name: "shelf",
      plural: false,
      selections: [
        v4 /*:: as any*/,
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "itemCount",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "updatedAt",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      concreteType: "ManifestationEdge",
      kind: "LinkedField",
      name: "edge",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "cursor",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "node",
          plural: false,
          selections: [v4 /*:: as any*/],
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    v7 = {
      kind: "InlineFragment",
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "message",
          storageKey: null,
        },
      ],
      type: "NotFound",
      abstractKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsAddShelfItemMutation",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "addShelfItem",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [v5 /*:: as any*/, v6 /*:: as any*/],
              type: "ShelfItemAdded",
              abstractKey: null,
            },
            v7 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "mutationsAddShelfItemMutation",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "addShelfItem",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                v5 /*:: as any*/,
                v6 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  filters: null,
                  handle: "appendEdge",
                  key: "",
                  kind: "LinkedHandle",
                  name: "edge",
                  handleArgs: [
                    {
                      kind: "Variable",
                      name: "connections",
                      variableName: "connections",
                    },
                  ],
                },
              ],
              type: "ShelfItemAdded",
              abstractKey: null,
            },
            v7 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "c874ff3af7a11366ecaf8345c856be6cc4ea5f867498a45b4a3ab7a0547e49a8",
      metadata: {},
      name: "mutationsAddShelfItemMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "227a01b32e0a33e053930fcd50173d96";

export default node;

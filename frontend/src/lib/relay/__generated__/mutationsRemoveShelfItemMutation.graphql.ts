/**
 * @generated SignedSource<<d63616fdad5dc3e7f5cbf54b64b349ec>>
 * @relayHash cdbfe1d0e0268f85ede1e68721ba7526
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 8df8a1083290a452e0a33e1b7d7ba2d604d6f0c9ff71a1f4342daa54ef9b3821

import { ConcreteRequest } from "relay-runtime";
export type mutationsRemoveShelfItemMutation$variables = {
  id: string;
  shelfId: string;
};
export type mutationsRemoveShelfItemMutation$data = {
  readonly removeShelfItem: {
    readonly manifestationId: string | null | undefined;
    readonly shelfId: string;
  };
};
export type mutationsRemoveShelfItemMutation = {
  response: mutationsRemoveShelfItemMutation$data;
  variables: mutationsRemoveShelfItemMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "shelfId",
    },
    v2 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "manifestationId",
            variableName: "id",
          },
          {
            kind: "Variable",
            name: "shelfId",
            variableName: "shelfId",
          },
        ],
        concreteType: "ShelfChange",
        kind: "LinkedField",
        name: "removeShelfItem",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "shelfId",
            storageKey: null,
          },
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "manifestationId",
            storageKey: null,
          },
        ],
        storageKey: null,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsRemoveShelfItemMutation",
      selections: v2 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "mutationsRemoveShelfItemMutation",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "8df8a1083290a452e0a33e1b7d7ba2d604d6f0c9ff71a1f4342daa54ef9b3821",
      metadata: {},
      name: "mutationsRemoveShelfItemMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "452af52dfc5e1cf6e1208fb69a954907";

export default node;

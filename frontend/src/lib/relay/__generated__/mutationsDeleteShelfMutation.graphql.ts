/**
 * @generated SignedSource<<812c7ca046fd850c51bd677df9fa73bd>>
 * @relayHash c51cecce191c9280f157ea2c1861b5b7
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 283f0e1a1cb5435db759a1fc5ec84f3000a905a3ade99dc48e0e1785fd3b5d10

import { ConcreteRequest } from "relay-runtime";
export type mutationsDeleteShelfMutation$variables = {
  shelfId: string;
};
export type mutationsDeleteShelfMutation$data = {
  readonly deleteShelf: {
    readonly shelfId: string;
  };
};
export type mutationsDeleteShelfMutation = {
  response: mutationsDeleteShelfMutation$data;
  variables: mutationsDeleteShelfMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "shelfId",
      },
    ],
    v1 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "id",
            variableName: "shelfId",
          },
        ],
        concreteType: "ShelfChange",
        kind: "LinkedField",
        name: "deleteShelf",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "shelfId",
            storageKey: null,
          },
        ],
        storageKey: null,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsDeleteShelfMutation",
      selections: v1 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsDeleteShelfMutation",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "283f0e1a1cb5435db759a1fc5ec84f3000a905a3ade99dc48e0e1785fd3b5d10",
      metadata: {},
      name: "mutationsDeleteShelfMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "5bca41eb0fa3014c6b43caa773b076de";

export default node;

/**
 * @generated SignedSource<<b7ea997dc419c38dff2e0537e3ad8e70>>
 * @relayHash 03b100749a1650d8ae8e189272e8638b
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID bb45a4b5c562000335fe8f63b015e51f79716d90776fe53ae73ecc8a6629f4d3

import { ConcreteRequest } from "relay-runtime";
export type mutationsRenameShelfMutation$variables = {
  name: string;
  shelfId: string;
};
export type mutationsRenameShelfMutation$data = {
  readonly renameShelf: {
    readonly createdAt: string;
    readonly id: string;
    readonly isSystem: boolean;
    readonly itemCount: number;
    readonly name: string;
    readonly updatedAt: string;
  };
};
export type mutationsRenameShelfMutation = {
  response: mutationsRenameShelfMutation$data;
  variables: mutationsRenameShelfMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "name",
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
            name: "id",
            variableName: "shelfId",
          },
          {
            kind: "Variable",
            name: "name",
            variableName: "name",
          },
        ],
        concreteType: "Shelf",
        kind: "LinkedField",
        name: "renameShelf",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "id",
            storageKey: null,
          },
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
            kind: "ScalarField",
            name: "updatedAt",
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
      name: "mutationsRenameShelfMutation",
      selections: v2 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "mutationsRenameShelfMutation",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "bb45a4b5c562000335fe8f63b015e51f79716d90776fe53ae73ecc8a6629f4d3",
      metadata: {},
      name: "mutationsRenameShelfMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "cc72dc9cc05e956db13a0326c2692cfa";

export default node;

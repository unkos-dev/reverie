/**
 * @generated SignedSource<<288b1b238fe8456aff772853a8784f11>>
 * @relayHash 462c44b9415fef35dd5523a97be1092d
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 7f9bf8d31a3218bb4b3fccbe34da565c99ead31da991f3b207bfee8a4ebd66f2

import { ConcreteRequest } from "relay-runtime";
export type mutationsAddShelfItemMutation$variables = {
  id: string;
  shelfId: string;
};
export type mutationsAddShelfItemMutation$data = {
  readonly addShelfItem: {
    readonly manifestationId: string | null | undefined;
    readonly shelfId: string;
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
        name: "addShelfItem",
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
      name: "mutationsAddShelfItemMutation",
      selections: v2 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "mutationsAddShelfItemMutation",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "7f9bf8d31a3218bb4b3fccbe34da565c99ead31da991f3b207bfee8a4ebd66f2",
      metadata: {},
      name: "mutationsAddShelfItemMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "da787625b39269882f295a872c0a7a70";

export default node;

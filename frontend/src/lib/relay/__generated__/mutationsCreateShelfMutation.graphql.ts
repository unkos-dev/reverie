/**
 * @generated SignedSource<<f12c3f5f8ff6c46f502bf083f876043d>>
 * @relayHash 82f4c37d1a8f1c8fb4f2cce993a15e24
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID caa002fc0b3b6478726524109ffb2352e79603e0cea192206ecdca0e5b8e27a3

import { ConcreteRequest } from "relay-runtime";
export type mutationsCreateShelfMutation$variables = {
  name: string;
};
export type mutationsCreateShelfMutation$data = {
  readonly createShelf: {
    readonly createdAt: string;
    readonly id: string;
    readonly isSystem: boolean;
    readonly itemCount: number;
    readonly name: string;
    readonly updatedAt: string;
  };
};
export type mutationsCreateShelfMutation = {
  response: mutationsCreateShelfMutation$data;
  variables: mutationsCreateShelfMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "name",
      },
    ],
    v1 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "name",
            variableName: "name",
          },
        ],
        concreteType: "Shelf",
        kind: "LinkedField",
        name: "createShelf",
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
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsCreateShelfMutation",
      selections: v1 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsCreateShelfMutation",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "caa002fc0b3b6478726524109ffb2352e79603e0cea192206ecdca0e5b8e27a3",
      metadata: {},
      name: "mutationsCreateShelfMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "ff1a89bfe7a7bdbdafd8f02fc831b718";

export default node;

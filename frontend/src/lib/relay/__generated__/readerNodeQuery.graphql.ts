/**
 * @generated SignedSource<<eca2e8b7744f3ae2f72088be284a469b>>
 * @relayHash cfd812abc3638a8299e61c573018f76a
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 42836234cc0b03f877da7515f2bd00e2c60b17372ee3b983c8c13c72195bf7bf

import { ConcreteRequest } from "relay-runtime";
export type readerNodeQuery$variables = {
  id: string;
};
export type readerNodeQuery$data = {
  readonly node:
    | {
        readonly id: string;
      }
    | null
    | undefined;
};
export type readerNodeQuery = {
  response: readerNodeQuery$data;
  variables: readerNodeQuery$variables;
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
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerNodeQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "node",
          plural: false,
          selections: [v2 /*:: as any*/],
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
      name: "readerNodeQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "node",
          plural: false,
          selections: [
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "__typename",
              storageKey: null,
            },
            v2 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "42836234cc0b03f877da7515f2bd00e2c60b17372ee3b983c8c13c72195bf7bf",
      metadata: {},
      name: "readerNodeQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "3742511597ef771707bab76ec72ee1ee";

export default node;

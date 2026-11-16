/**
 * @generated SignedSource<<3ac55ac7e38ac2c42f8a6b0ed9a1233f>>
 * @relayHash 48cea04ea1e1a31d592ee457f23170f4
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 5bbd7f06e5c7371398babe87129740e7468e7333c41dd8678995db67bce93b2b

import { ConcreteRequest } from "relay-runtime";
export type mutationsRefreshMetadataMutation$variables = {
  id: string;
};
export type mutationsRefreshMetadataMutation$data = {
  readonly refreshMetadata: {
    readonly manifestationId: string;
  };
};
export type mutationsRefreshMetadataMutation = {
  response: mutationsRefreshMetadataMutation$data;
  variables: mutationsRefreshMetadataMutation$variables;
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
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "id",
            variableName: "id",
          },
        ],
        concreteType: "MetadataRefresh",
        kind: "LinkedField",
        name: "refreshMetadata",
        plural: false,
        selections: [
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
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsRefreshMetadataMutation",
      selections: v1 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsRefreshMetadataMutation",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "5bbd7f06e5c7371398babe87129740e7468e7333c41dd8678995db67bce93b2b",
      metadata: {},
      name: "mutationsRefreshMetadataMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "2c9fa96f6ead49cdc65e972c31acd163";

export default node;

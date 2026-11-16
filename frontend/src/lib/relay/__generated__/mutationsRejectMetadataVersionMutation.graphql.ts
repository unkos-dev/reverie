/**
 * @generated SignedSource<<96dfc883e24e4a6e615b28e6a1138509>>
 * @relayHash 76e326e7f108abf631cc6a661c806cce
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 28c19c978aba985ed5f10f304fd35a6c9d93141a3317b95b1802c75c2b06e16a

import { ConcreteRequest } from "relay-runtime";
export type mutationsRejectMetadataVersionMutation$variables = {
  id: string;
  versionId: string;
};
export type mutationsRejectMetadataVersionMutation$data = {
  readonly rejectMetadataVersion: {
    readonly manifestationId: string;
  };
};
export type mutationsRejectMetadataVersionMutation = {
  response: mutationsRejectMetadataVersionMutation$data;
  variables: mutationsRejectMetadataVersionMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "id",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "versionId",
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
          {
            kind: "Variable",
            name: "versionId",
            variableName: "versionId",
          },
        ],
        concreteType: "MetadataReview",
        kind: "LinkedField",
        name: "rejectMetadataVersion",
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
      name: "mutationsRejectMetadataVersionMutation",
      selections: v1 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsRejectMetadataVersionMutation",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "28c19c978aba985ed5f10f304fd35a6c9d93141a3317b95b1802c75c2b06e16a",
      metadata: {},
      name: "mutationsRejectMetadataVersionMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "cdd0eb3aff2bc04da526ebe791f38521";

export default node;

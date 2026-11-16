/**
 * @generated SignedSource<<b09bc14becc398d539ea639625a8a345>>
 * @relayHash 7e24926942755cf77c4599bacd768170
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 36bbdd65ffd84bbaafaf6f0ede895b38fcede2ae5e03c739e63d6d3ab8d22ccd

import { ConcreteRequest } from "relay-runtime";
export type mutationsLockMetadataFieldMutation$variables = {
  entityType: string;
  fieldName: string;
  id: string;
};
export type mutationsLockMetadataFieldMutation$data = {
  readonly lockMetadataField: {
    readonly manifestationId: string;
  };
};
export type mutationsLockMetadataFieldMutation = {
  response: mutationsLockMetadataFieldMutation$data;
  variables: mutationsLockMetadataFieldMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "entityType",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "fieldName",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v3 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "entityType",
            variableName: "entityType",
          },
          {
            kind: "Variable",
            name: "fieldName",
            variableName: "fieldName",
          },
          {
            kind: "Variable",
            name: "id",
            variableName: "id",
          },
        ],
        concreteType: "MetadataReview",
        kind: "LinkedField",
        name: "lockMetadataField",
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
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsLockMetadataFieldMutation",
      selections: v3 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v2 /*:: as any*/, v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "mutationsLockMetadataFieldMutation",
      selections: v3 /*:: as any*/,
    },
    params: {
      id: "36bbdd65ffd84bbaafaf6f0ede895b38fcede2ae5e03c739e63d6d3ab8d22ccd",
      metadata: {},
      name: "mutationsLockMetadataFieldMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "af0ad82fce1c837ffa307eb2d7dafd47";

export default node;

/**
 * @generated SignedSource<<e96d12a55924993e4676afda7a8060c6>>
 * @relayHash b6aaa33cdb9ca57316c50b9e7ac09803
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID f80c617cdac6d2f154fae96f761ba7c8dbcf1e9e4e59b9196738a45e7fbedc76

import { ConcreteRequest } from "relay-runtime";
export type mutationsUnlockMetadataFieldMutation$variables = {
  entityType: string;
  fieldName: string;
  id: string;
};
export type mutationsUnlockMetadataFieldMutation$data = {
  readonly unlockMetadataField: {
    readonly manifestationId: string;
  };
};
export type mutationsUnlockMetadataFieldMutation = {
  response: mutationsUnlockMetadataFieldMutation$data;
  variables: mutationsUnlockMetadataFieldMutation$variables;
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
        name: "unlockMetadataField",
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
      name: "mutationsUnlockMetadataFieldMutation",
      selections: v3 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v2 /*:: as any*/, v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "mutationsUnlockMetadataFieldMutation",
      selections: v3 /*:: as any*/,
    },
    params: {
      id: "f80c617cdac6d2f154fae96f761ba7c8dbcf1e9e4e59b9196738a45e7fbedc76",
      metadata: {},
      name: "mutationsUnlockMetadataFieldMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "6c07296fcf9fe506b79a94cd380fc988";

export default node;

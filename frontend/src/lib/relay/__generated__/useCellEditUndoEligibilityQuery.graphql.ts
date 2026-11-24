/**
 * @generated SignedSource<<58f36dd67cb1ed1fa7520db4dadc9fdc>>
 * @relayHash 2ff67d68394f08be161b3c6533378cba
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID fb2e37ccd4d6ce3e26351dea683b177fdd6467d7edd697904917feb7f9fb076f

import { ConcreteRequest } from "relay-runtime";
export type FieldOperationStatus =
  | "CONSUMED"
  | "ELIGIBLE"
  | "LOCKED"
  | "UNAVAILABLE"
  | "%future added value";
export type useCellEditUndoEligibilityQuery$variables = {
  id: string;
  operationId: string;
};
export type useCellEditUndoEligibilityQuery$data = {
  readonly metadataOperation: {
    readonly metadata: {
      readonly etag: string;
    };
    readonly operations: ReadonlyArray<{
      readonly operationId: string;
      readonly status: FieldOperationStatus;
    }>;
  };
};
export type useCellEditUndoEligibilityQuery = {
  response: useCellEditUndoEligibilityQuery$data;
  variables: useCellEditUndoEligibilityQuery$variables;
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
        name: "operationId",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
      {
        kind: "Variable",
        name: "operationId",
        variableName: "operationId",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      concreteType: "FieldOperation",
      kind: "LinkedField",
      name: "operations",
      plural: true,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "operationId",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "status",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "useCellEditUndoEligibilityQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "MetadataOperation",
          kind: "LinkedField",
          name: "metadataOperation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [v3 /*:: as any*/],
              storageKey: null,
            },
          ],
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
      name: "useCellEditUndoEligibilityQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "MetadataOperation",
          kind: "LinkedField",
          name: "metadataOperation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [
                v3 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "id",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "fb2e37ccd4d6ce3e26351dea683b177fdd6467d7edd697904917feb7f9fb076f",
      metadata: {},
      name: "useCellEditUndoEligibilityQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "6f2a7cb21ac31d4f67db80df9b23f954";

export default node;

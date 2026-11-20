/**
 * @generated SignedSource<<d3520efe44531642f783d5785d35b307>>
 * @relayHash da5efc13eb8f170837d3b036f1cd0f98
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID b85e6db0492a05a207cfbe41cbdc23d15ce9b5db666540dadd1f5dd78a9adaa6

import { ConcreteRequest } from "relay-runtime";
export type mutationsUndoTitleMutation$variables = {
  baseline?: string | null | undefined;
  id: string;
  operationId: string;
};
export type mutationsUndoTitleMutation$data = {
  readonly undoMetadata: {
    readonly metadata: {
      readonly etag: string;
      readonly id: string;
      readonly manifestationId: string;
      readonly title: string;
      readonly workId: string;
    };
    readonly operations: ReadonlyArray<{
      readonly expiresAt: string;
      readonly fieldName: string;
      readonly operationId: string;
      readonly status: string;
    }>;
    readonly outcome: string;
  };
};
export type mutationsUndoTitleMutation = {
  response: mutationsUndoTitleMutation$data;
  variables: mutationsUndoTitleMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "baseline",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "operationId",
    },
    v3 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "baseline",
            variableName: "baseline",
          },
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
        concreteType: "MetadataOperation",
        kind: "LinkedField",
        name: "undoMetadata",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "outcome",
            storageKey: null,
          },
          {
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
                name: "fieldName",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "expiresAt",
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
          {
            alias: null,
            args: null,
            concreteType: "MetadataRevision",
            kind: "LinkedField",
            name: "metadata",
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
                name: "workId",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "manifestationId",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "etag",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "title",
                storageKey: null,
              },
            ],
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
      name: "mutationsUndoTitleMutation",
      selections: v3 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v2 /*:: as any*/],
      kind: "Operation",
      name: "mutationsUndoTitleMutation",
      selections: v3 /*:: as any*/,
    },
    params: {
      id: "b85e6db0492a05a207cfbe41cbdc23d15ce9b5db666540dadd1f5dd78a9adaa6",
      metadata: {},
      name: "mutationsUndoTitleMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "ee786bea3e31de80513f4e96fe2e60df";

export default node;

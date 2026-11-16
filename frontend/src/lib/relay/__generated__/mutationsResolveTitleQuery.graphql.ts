/**
 * @generated SignedSource<<42da8eddf0fd4c0d2a141595bcb5995e>>
 * @relayHash dafe7c54c48f9ebd89b200e6489d9fbf
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 7b9ad7b7566df37bf3b05f7a8a4e5004dcd8d8fb50b57fe5c40dd9e585ed49ee

import { ConcreteRequest } from "relay-runtime";
export type mutationsResolveTitleQuery$variables = {
  id: string;
  operationId?: string | null | undefined;
  requestId?: string | null | undefined;
};
export type mutationsResolveTitleQuery$data = {
  readonly metadataOperation: {
    readonly metadata: {
      readonly etag: string;
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
export type mutationsResolveTitleQuery = {
  response: mutationsResolveTitleQuery$data;
  variables: mutationsResolveTitleQuery$variables;
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
      name: "operationId",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "requestId",
    },
    v3 = [
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
            name: "operationId",
            variableName: "operationId",
          },
          {
            kind: "Variable",
            name: "requestId",
            variableName: "requestId",
          },
        ],
        concreteType: "MetadataOperation",
        kind: "LinkedField",
        name: "metadataOperation",
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
            concreteType: "MetadataSnapshot",
            kind: "LinkedField",
            name: "metadata",
            plural: false,
            selections: [
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
      name: "mutationsResolveTitleQuery",
      selections: v3 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v0 /*:: as any*/, v2 /*:: as any*/, v1 /*:: as any*/],
      kind: "Operation",
      name: "mutationsResolveTitleQuery",
      selections: v3 /*:: as any*/,
    },
    params: {
      id: "7b9ad7b7566df37bf3b05f7a8a4e5004dcd8d8fb50b57fe5c40dd9e585ed49ee",
      metadata: {},
      name: "mutationsResolveTitleQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "7309f6c58f2a9b5bb7a7515dcda46327";

export default node;

/**
 * @generated SignedSource<<89862bc6fb5787d728979b1294058abf>>
 * @relayHash 24df2dd06d40ebefbafc14a909aa5817
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 832cd3625c7d0bbfc1b061f097755dac3c7e7e3e9f3b973de505e9df2a4714d4

import { ConcreteRequest } from "relay-runtime";
export type mutationsResolveCellsQuery$variables = {
  id: string;
  operationId?: string | null | undefined;
  requestId?: string | null | undefined;
};
export type mutationsResolveCellsQuery$data = {
  readonly metadataOperation: {
    readonly metadata: {
      readonly etag: string;
      readonly isbn13: string | null | undefined;
      readonly manifestationId: string;
      readonly pages: number | null | undefined;
      readonly subtitle: string | null | undefined;
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
export type mutationsResolveCellsQuery = {
  response: mutationsResolveCellsQuery$data;
  variables: mutationsResolveCellsQuery$variables;
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
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "subtitle",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "isbn13",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "pages",
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
      name: "mutationsResolveCellsQuery",
      selections: v3 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v0 /*:: as any*/, v2 /*:: as any*/, v1 /*:: as any*/],
      kind: "Operation",
      name: "mutationsResolveCellsQuery",
      selections: v3 /*:: as any*/,
    },
    params: {
      id: "832cd3625c7d0bbfc1b061f097755dac3c7e7e3e9f3b973de505e9df2a4714d4",
      metadata: {},
      name: "mutationsResolveCellsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "80ea97b4869439db634d59a9ac675346";

export default node;

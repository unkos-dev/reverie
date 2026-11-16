/**
 * @generated SignedSource<<25022dd4d0602cf805c0f40f9e9000ed>>
 * @relayHash ecf58f90b24f8ac92f73e10658eb1a90
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID da59e05805993815f680c01e9d6742914e65e521f444c784614559f6e587a2f2

import { ConcreteRequest } from "relay-runtime";
export type mutationsCellsQuery$variables = {
  id: string;
};
export type mutationsCellsQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly metadata: {
          readonly etag: string;
          readonly isbn13: string | null | undefined;
          readonly manifestationId: string;
          readonly pages: number | null | undefined;
          readonly subtitle: string | null | undefined;
          readonly title: string;
          readonly workId: string;
        };
      }
    | null
    | undefined;
};
export type mutationsCellsQuery = {
  response: mutationsCellsQuery$data;
  variables: mutationsCellsQuery$variables;
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
        concreteType: "Manifestation",
        kind: "LinkedField",
        name: "manifestation",
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
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsCellsQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsCellsQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "da59e05805993815f680c01e9d6742914e65e521f444c784614559f6e587a2f2",
      metadata: {},
      name: "mutationsCellsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "422d413e3b910b190309c3d2f499b0b0";

export default node;

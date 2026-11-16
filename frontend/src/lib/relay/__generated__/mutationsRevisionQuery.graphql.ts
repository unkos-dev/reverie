/**
 * @generated SignedSource<<daa62c36b446c701e9ce9cc5cdb91f25>>
 * @relayHash 54e768fb9d6a245284529706a6965d06
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 511d0350c5790ce6a2f4600d9750a94b18c45bea30988cbaf7eb11f0ae7a1131

import { ConcreteRequest } from "relay-runtime";
export type mutationsRevisionQuery$variables = {
  id: string;
};
export type mutationsRevisionQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly metadata: {
          readonly etag: string;
          readonly manifestationId: string;
          readonly workId: string;
        };
      }
    | null
    | undefined;
};
export type mutationsRevisionQuery = {
  response: mutationsRevisionQuery$data;
  variables: mutationsRevisionQuery$variables;
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
      name: "mutationsRevisionQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsRevisionQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "511d0350c5790ce6a2f4600d9750a94b18c45bea30988cbaf7eb11f0ae7a1131",
      metadata: {},
      name: "mutationsRevisionQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "6bf0fbf7a589e22ed5524869c6139351";

export default node;

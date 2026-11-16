/**
 * @generated SignedSource<<2f2f497eefc45390fa0f801f238d41bd>>
 * @relayHash 3ba2ab1fb6129f9f02e63962dd2ffe4a
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 5cd8056a2368a9abe2d747ae77027c73a69c90ab88af97c4d1227326560fcf7b

import { ConcreteRequest } from "relay-runtime";
export type mutationsTitleQuery$variables = {
  id: string;
};
export type mutationsTitleQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly metadata: {
          readonly etag: string;
          readonly manifestationId: string;
          readonly title: string;
          readonly workId: string;
        };
      }
    | null
    | undefined;
};
export type mutationsTitleQuery = {
  response: mutationsTitleQuery$data;
  variables: mutationsTitleQuery$variables;
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
      name: "mutationsTitleQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsTitleQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "5cd8056a2368a9abe2d747ae77027c73a69c90ab88af97c4d1227326560fcf7b",
      metadata: {},
      name: "mutationsTitleQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "32503f33a991a31d63c9a287fa894dd7";

export default node;

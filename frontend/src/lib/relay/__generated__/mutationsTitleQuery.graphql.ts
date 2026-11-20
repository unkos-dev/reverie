/**
 * @generated SignedSource<<1acc0ea771d32db6623aabd91ffb9adc>>
 * @relayHash 6355b9b58fbb02f99511809c8fa11c5e
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 941ae2c1b433f1a2f53fc0792294e9caaa0ea390a172cc0609bb639ba16cfad7

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
          readonly id: string;
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
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v2 = [
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
          v1 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "MetadataRevision",
            kind: "LinkedField",
            name: "metadata",
            plural: false,
            selections: [
              v1 /*:: as any*/,
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
      selections: v2 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsTitleQuery",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "941ae2c1b433f1a2f53fc0792294e9caaa0ea390a172cc0609bb639ba16cfad7",
      metadata: {},
      name: "mutationsTitleQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "32503f33a991a31d63c9a287fa894dd7";

export default node;

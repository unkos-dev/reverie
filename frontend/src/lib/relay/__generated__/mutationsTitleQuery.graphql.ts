/**
 * @generated SignedSource<<8ebed972251b141571eef84119521526>>
 * @relayHash 3040bebe5740455a9eaef157278b3e6d
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID a016e4afbead8246e91215bd484b39b1d717ae94bb1328bfe55488fa44fda09d

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
          readonly title: string;
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
      id: "a016e4afbead8246e91215bd484b39b1d717ae94bb1328bfe55488fa44fda09d",
      metadata: {},
      name: "mutationsTitleQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "32503f33a991a31d63c9a287fa894dd7";

export default node;

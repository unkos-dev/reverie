/**
 * @generated SignedSource<<35c9e08b81a4355f4b7962a590a0c6ee>>
 * @relayHash bbb6c991f893518407937bd084f178ab
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 55788566d0a32b7a003a8c9f9814a39de0b87a13b9e4572325089393f85f971b

import { ConcreteRequest } from "relay-runtime";
export type mutationsReadingQuery$variables = {
  id: string;
};
export type mutationsReadingQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly readingState: {
          readonly etag: string;
          readonly finishedAt: string | null | undefined;
          readonly lastReadAt: string | null | undefined;
          readonly notes: string | null | undefined;
          readonly progressPct: number | null | undefined;
          readonly rating: number | null | undefined;
          readonly startedAt: string | null | undefined;
          readonly status: string | null | undefined;
        };
      }
    | null
    | undefined;
};
export type mutationsReadingQuery = {
  response: mutationsReadingQuery$data;
  variables: mutationsReadingQuery$variables;
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
            concreteType: "ReadingState",
            kind: "LinkedField",
            name: "readingState",
            plural: false,
            selections: [
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
                name: "status",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "rating",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "notes",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "progressPct",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "startedAt",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "finishedAt",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "lastReadAt",
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
      name: "mutationsReadingQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsReadingQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "55788566d0a32b7a003a8c9f9814a39de0b87a13b9e4572325089393f85f971b",
      metadata: {},
      name: "mutationsReadingQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "5fc576a577e5127ce6b314c442c9f580";

export default node;

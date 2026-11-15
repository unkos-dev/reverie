/**
 * @generated SignedSource<<e61d912d5d017141925288d972930b81>>
 * @relayHash d582f7013608398d2f05e41bba688c4f
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 0c20263c1ded391dc657a35b478d8be57c0fab91f15375c66662a3650a965cfd

import { ConcreteRequest } from "relay-runtime";
export type readerVersionsQuery$variables = {
  id: string;
};
export type readerVersionsQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly versions: {
          readonly acceptedCount: number;
          readonly pending: ReadonlyArray<{
            readonly confidenceScore: number;
            readonly fieldName: string;
            readonly id: string;
            readonly matchType: string;
            readonly newValue: any;
            readonly observationCount: number;
            readonly source: string;
            readonly status: string;
          }>;
          readonly pendingCount: number;
        };
      }
    | null
    | undefined;
};
export type readerVersionsQuery = {
  response: readerVersionsQuery$data;
  variables: readerVersionsQuery$variables;
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
            concreteType: "MetadataVersions",
            kind: "LinkedField",
            name: "versions",
            plural: false,
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "pendingCount",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "acceptedCount",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                concreteType: "MetadataVersion",
                kind: "LinkedField",
                name: "pending",
                plural: true,
                selections: [
                  v1 /*:: as any*/,
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
                    name: "source",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "newValue",
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
                    name: "confidenceScore",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "matchType",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "observationCount",
                    storageKey: null,
                  },
                ],
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
      name: "readerVersionsQuery",
      selections: v2 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerVersionsQuery",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "0c20263c1ded391dc657a35b478d8be57c0fab91f15375c66662a3650a965cfd",
      metadata: {},
      name: "readerVersionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "fcea35dd4e874d8adf26ce0fce59125f";

export default node;

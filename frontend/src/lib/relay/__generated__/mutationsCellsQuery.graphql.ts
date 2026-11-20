/**
 * @generated SignedSource<<cf1b3e6dbced9c29ac6b4cf711bd1d9b>>
 * @relayHash f5a99e324d10014b404709bffdba97fc
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 1387bbe2a013b26043bf6d3f7477ad5104bba44a888d5100f2fd9b1a94982dec

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
          readonly id: string;
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
      selections: v2 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsCellsQuery",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "1387bbe2a013b26043bf6d3f7477ad5104bba44a888d5100f2fd9b1a94982dec",
      metadata: {},
      name: "mutationsCellsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "422d413e3b910b190309c3d2f499b0b0";

export default node;

/**
 * @generated SignedSource<<8ba58a2717ecd48e43943a51520465cf>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerVersions$data = {
  readonly id: string;
  readonly versions: {
    readonly pending: {
      readonly edges: ReadonlyArray<{
        readonly node: {
          readonly confidenceScore: number;
          readonly fieldName: string;
          readonly id: string;
          readonly matchType: string;
          readonly newValue: unknown;
          readonly observationCount: number;
          readonly source: string;
          readonly status: string;
        };
      }>;
    };
    readonly pendingCount: number;
  };
  readonly " $fragmentType": "readerVersions";
};
export type readerVersions$key = {
  readonly " $data"?: readerVersions$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerVersions">;
};

import readerVersionsPaginationQuery_graphql from "./readerVersionsPaginationQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = ["versions", "pending"],
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    };
  return {
    argumentDefinitions: [
      {
        defaultValue: 20,
        kind: "LocalArgument",
        name: "count",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "cursor",
      },
    ],
    kind: "Fragment",
    metadata: {
      connection: [
        {
          count: "count",
          cursor: "cursor",
          direction: "forward",
          path: v0 /*:: as any*/,
        },
      ],
      refetch: {
        connection: {
          forward: {
            count: "count",
            cursor: "cursor",
          },
          backward: null,
          path: v0 /*:: as any*/,
        },
        fragmentPathInResult: ["fetch__Manifestation"],
        operation: readerVersionsPaginationQuery_graphql,
        identifierInfo: {
          identifierField: "id",
          identifierQueryVariableName: "id",
        },
      },
      throwOnFieldError: true,
    },
    name: "readerVersions",
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
            alias: "pending",
            args: null,
            concreteType: "MetadataVersionConnection",
            kind: "LinkedField",
            name: "__readerVersions_pending_connection",
            plural: false,
            selections: [
              {
                alias: null,
                args: null,
                concreteType: "MetadataVersionEdge",
                kind: "LinkedField",
                name: "edges",
                plural: true,
                selections: [
                  {
                    alias: null,
                    args: null,
                    concreteType: "MetadataVersion",
                    kind: "LinkedField",
                    name: "node",
                    plural: false,
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
                        name: "status",
                        storageKey: null,
                      },
                      {
                        alias: null,
                        args: null,
                        kind: "ScalarField",
                        name: "observationCount",
                        storageKey: null,
                      },
                      {
                        alias: null,
                        args: null,
                        kind: "ScalarField",
                        name: "__typename",
                        storageKey: null,
                      },
                    ],
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "cursor",
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                concreteType: "PageInfo",
                kind: "LinkedField",
                name: "pageInfo",
                plural: false,
                selections: [
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "endCursor",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "hasNextPage",
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
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "0ca8df1a57d195115b99e4405134a87f";

export default node;

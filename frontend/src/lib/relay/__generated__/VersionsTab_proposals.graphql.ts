/**
 * @generated SignedSource<<91b65514a44e7311f7c1a102175da553>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type VersionsTab_proposals$data = {
  readonly id: string;
  readonly versions: {
    readonly __id: string;
    readonly pending: {
      readonly edges: ReadonlyArray<{
        readonly node: {
          readonly fieldName: string;
          readonly id: string;
          readonly " $fragmentSpreads": FragmentRefs<"VersionsTab_proposal">;
        };
      }>;
    };
    readonly pendingCount: number;
  };
  readonly " $fragmentType": "VersionsTab_proposals";
};
export type VersionsTab_proposals$key = {
  readonly " $data"?: VersionsTab_proposals$data;
  readonly " $fragmentSpreads": FragmentRefs<"VersionsTab_proposals">;
};

import VersionsTabPaginationQuery_graphql from "./VersionsTabPaginationQuery.graphql";

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
        operation: VersionsTabPaginationQuery_graphql,
        identifierInfo: {
          identifierField: "id",
          identifierQueryVariableName: "id",
        },
      },
      throwOnFieldError: true,
    },
    name: "VersionsTab_proposals",
    selections: [
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
            name: "__VersionsTab_proposals_pending_connection",
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
                        args: null,
                        kind: "FragmentSpread",
                        name: "VersionsTab_proposal",
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
          {
            kind: "ClientExtension",
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "__id",
                storageKey: null,
              },
            ],
          },
        ],
        storageKey: null,
      },
      v1 /*:: as any*/,
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "de638bcb620ced14b906fe4343de9079";

export default node;

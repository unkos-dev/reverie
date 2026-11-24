/**
 * @generated SignedSource<<54227ad8301ff87e73b2e8fc8f714bc0>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ShelfDetailPage_items$data = {
  readonly id: string;
  readonly items: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly id: string;
      };
    }>;
  };
  readonly " $fragmentType": "ShelfDetailPage_items";
};
export type ShelfDetailPage_items$key = {
  readonly " $data"?: ShelfDetailPage_items$data;
  readonly " $fragmentSpreads": FragmentRefs<"ShelfDetailPage_items">;
};

import ShelfDetailPageItemsPaginationQuery_graphql from "./ShelfDetailPageItemsPaginationQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = ["items"],
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
        defaultValue: 50,
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
        fragmentPathInResult: ["fetch__Shelf"],
        operation: ShelfDetailPageItemsPaginationQuery_graphql,
        identifierInfo: {
          identifierField: "id",
          identifierQueryVariableName: "id",
        },
      },
      throwOnFieldError: true,
    },
    name: "ShelfDetailPage_items",
    selections: [
      {
        alias: "items",
        args: null,
        concreteType: "ManifestationConnection",
        kind: "LinkedField",
        name: "__ShelfDetailPage_items_connection",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "ManifestationEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              {
                alias: null,
                args: null,
                concreteType: "Manifestation",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: [
                  v1 /*:: as any*/,
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
      v1 /*:: as any*/,
    ],
    type: "Shelf",
    abstractKey: null,
  };
})();

(node as any).hash = "b0da6e5a328ad2e7beb0fe2283586d18";

export default node;

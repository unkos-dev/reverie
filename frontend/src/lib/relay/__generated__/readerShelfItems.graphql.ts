/**
 * @generated SignedSource<<344e03fcca96a73d5fe6f82cf0e2461a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelfItems$data = {
  readonly id: string;
  readonly items: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly id: string;
      };
    }>;
  };
  readonly " $fragmentType": "readerShelfItems";
};
export type readerShelfItems$key = {
  readonly " $data"?: readerShelfItems$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerShelfItems">;
};

import readerShelfItemsPaginationQuery_graphql from "./readerShelfItemsPaginationQuery.graphql";

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
        operation: readerShelfItemsPaginationQuery_graphql,
        identifierInfo: {
          identifierField: "id",
          identifierQueryVariableName: "id",
        },
      },
      throwOnFieldError: true,
    },
    name: "readerShelfItems",
    selections: [
      {
        alias: "items",
        args: null,
        concreteType: "ManifestationConnection",
        kind: "LinkedField",
        name: "__readerShelfItems_items_connection",
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

(node as any).hash = "f366d894eff54bc4877dd3881a7fb59f";

export default node;

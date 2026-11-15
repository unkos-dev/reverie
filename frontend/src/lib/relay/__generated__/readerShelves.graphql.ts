/**
 * @generated SignedSource<<b8d1da1906822c0f776fce99cfb2d8ae>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelves$data = {
  readonly shelves: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly id: string;
        readonly isSystem: boolean;
        readonly itemCount: number;
        readonly name: string;
      };
    }>;
  };
  readonly " $fragmentType": "readerShelves";
};
export type readerShelves$key = {
  readonly " $data"?: readerShelves$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerShelves">;
};

import readerShelvesPaginationQuery_graphql from "./readerShelvesPaginationQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = ["shelves"];
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
        fragmentPathInResult: [],
        operation: readerShelvesPaginationQuery_graphql,
      },
    },
    name: "readerShelves",
    selections: [
      {
        alias: "shelves",
        args: null,
        concreteType: "ShelfConnection",
        kind: "LinkedField",
        name: "__readerShelves_shelves_connection",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "ShelfEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              {
                alias: null,
                args: null,
                concreteType: "Shelf",
                kind: "LinkedField",
                name: "node",
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
                    kind: "ScalarField",
                    name: "name",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "isSystem",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "itemCount",
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
    type: "Query",
    abstractKey: null,
  };
})();

(node as any).hash = "4f9978accc7438b3879197f17a462d76";

export default node;

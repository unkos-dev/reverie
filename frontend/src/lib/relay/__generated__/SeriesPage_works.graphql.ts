/**
 * @generated SignedSource<<6d230bbdfde4462165dde135c9f16a6c>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type SeriesPage_works$data = {
  readonly id: string;
  readonly works: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly id: string;
        readonly " $fragmentSpreads": FragmentRefs<"SeriesPage_work">;
      };
      readonly position: number | null | undefined;
    }>;
  };
  readonly " $fragmentType": "SeriesPage_works";
};
export type SeriesPage_works$key = {
  readonly " $data"?: SeriesPage_works$data;
  readonly " $fragmentSpreads": FragmentRefs<"SeriesPage_works">;
};

import SeriesPageWorksPaginationQuery_graphql from "./SeriesPageWorksPaginationQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = ["works"],
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
        fragmentPathInResult: ["fetch__Series"],
        operation: SeriesPageWorksPaginationQuery_graphql,
        identifierInfo: {
          identifierField: "id",
          identifierQueryVariableName: "id",
        },
      },
      throwOnFieldError: true,
    },
    name: "SeriesPage_works",
    selections: [
      {
        alias: "works",
        args: null,
        concreteType: "WorkConnection",
        kind: "LinkedField",
        name: "__SeriesPage_works_connection",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "WorkEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "position",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                concreteType: "Work",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: [
                  v1 /*:: as any*/,
                  {
                    args: null,
                    kind: "FragmentSpread",
                    name: "SeriesPage_work",
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
      v1 /*:: as any*/,
    ],
    type: "Series",
    abstractKey: null,
  };
})();

(node as any).hash = "428c8f76bc2be8a82ab38bd7ea023eb7";

export default node;

/**
 * @generated SignedSource<<f6f574f1ec4d694b2dae8674f634a198>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerWorkAuthors$data = {
  readonly authors: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly id: string;
        readonly name: string;
      };
    }>;
  };
  readonly id: string;
  readonly " $fragmentType": "readerWorkAuthors";
};
export type readerWorkAuthors$key = {
  readonly " $data"?: readerWorkAuthors$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerWorkAuthors">;
};

import readerWorkAuthorsPaginationQuery_graphql from "./readerWorkAuthorsPaginationQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = ["authors"],
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
        fragmentPathInResult: ["fetch__Work"],
        operation: readerWorkAuthorsPaginationQuery_graphql,
        identifierInfo: {
          identifierField: "id",
          identifierQueryVariableName: "id",
        },
      },
      throwOnFieldError: true,
    },
    name: "readerWorkAuthors",
    selections: [
      v1 /*:: as any*/,
      {
        alias: "authors",
        args: null,
        concreteType: "AuthorConnection",
        kind: "LinkedField",
        name: "__readerWorkAuthors_authors_connection",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "AuthorEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              {
                alias: null,
                args: null,
                concreteType: "Author",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: [
                  v1 /*:: as any*/,
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
    type: "Work",
    abstractKey: null,
  };
})();

(node as any).hash = "5093d7000cbe67ab4c7253178d3d2f18";

export default node;

/**
 * @generated SignedSource<<54404b64628c74123ae2dc99e93e13bd>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type LibraryPage_rows$data = {
  readonly books: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly id: string;
        readonly metadata: {
          readonly title: string;
        };
        readonly " $fragmentSpreads": FragmentRefs<
          "BookCard_book" | "LibraryTable_rows" | "useCellEdit_batchRows"
        >;
      };
    }>;
  };
  readonly " $fragmentType": "LibraryPage_rows";
};
export type LibraryPage_rows$key = {
  readonly " $data"?: LibraryPage_rows$data;
  readonly " $fragmentSpreads": FragmentRefs<"LibraryPage_rows">;
};

import LibraryPageRowsPaginationQuery_graphql from "./LibraryPageRowsPaginationQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = ["books"];
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
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "filter",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "sort",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "sourceKey",
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
        operation: LibraryPageRowsPaginationQuery_graphql,
      },
      throwOnFieldError: true,
    },
    name: "LibraryPage_rows",
    selections: [
      {
        alias: "books",
        args: [
          {
            kind: "Variable",
            name: "__dynamicKey",
            variableName: "sourceKey",
          },
          {
            kind: "Variable",
            name: "filter",
            variableName: "filter",
          },
          {
            kind: "Variable",
            name: "sort",
            variableName: "sort",
          },
        ],
        concreteType: "ManifestationConnection",
        kind: "LinkedField",
        name: "__LibraryPage_rows_books_connection",
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
                    concreteType: "MetadataRevision",
                    kind: "LinkedField",
                    name: "metadata",
                    plural: false,
                    selections: [
                      {
                        alias: null,
                        args: null,
                        kind: "ScalarField",
                        name: "title",
                        storageKey: null,
                      },
                    ],
                    storageKey: null,
                  },
                  {
                    args: null,
                    kind: "FragmentSpread",
                    name: "LibraryTable_rows",
                  },
                  {
                    args: null,
                    kind: "FragmentSpread",
                    name: "BookCard_book",
                  },
                  {
                    args: null,
                    kind: "FragmentSpread",
                    name: "useCellEdit_batchRows",
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

(node as any).hash = "cb6cdfe277737fd46661ac1c1e0a779a";

export default node;

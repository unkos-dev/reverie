/**
 * @generated SignedSource<<d3d827488d73479f25a1b51c61be12d0>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerLibrary$data = {
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
  readonly " $fragmentType": "readerLibrary";
};
export type readerLibrary$key = {
  readonly " $data"?: readerLibrary$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerLibrary">;
};

import readerLibraryPaginationQuery_graphql from "./readerLibraryPaginationQuery.graphql";

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
        operation: readerLibraryPaginationQuery_graphql,
      },
      throwOnFieldError: true,
    },
    name: "readerLibrary",
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
        name: "__readerLibrary_books_connection",
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

(node as any).hash = "b8c0363a5d29ecb7229cc4da79d8e48d";

export default node;

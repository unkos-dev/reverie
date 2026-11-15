/**
 * @generated SignedSource<<8bf33f352e65ce1a83b584458fbbf6b2>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSeriesWorks$data = {
  readonly id: string;
  readonly works: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly authors: {
          readonly edges: ReadonlyArray<{
            readonly node: {
              readonly name: string;
            };
          }>;
          readonly pageInfo: {
            readonly endCursor: string | null | undefined;
            readonly hasNextPage: boolean;
          };
        };
        readonly id: string;
        readonly manifestations: {
          readonly edges: ReadonlyArray<{
            readonly node: {
              readonly coverUrl: string;
              readonly id: string;
            };
          }>;
          readonly pageInfo: {
            readonly endCursor: string | null | undefined;
            readonly hasNextPage: boolean;
          };
        };
        readonly title: string;
      };
      readonly position: number | null | undefined;
    }>;
  };
  readonly " $fragmentType": "readerSeriesWorks";
};
export type readerSeriesWorks$key = {
  readonly " $data"?: readerSeriesWorks$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerSeriesWorks">;
};

import readerSeriesWorksPaginationQuery_graphql from "./readerSeriesWorksPaginationQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = ["works"],
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hasNextPage",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "endCursor",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      concreteType: "PageInfo",
      kind: "LinkedField",
      name: "pageInfo",
      plural: false,
      selections: [v2 /*:: as any*/, v3 /*:: as any*/],
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
        operation: readerSeriesWorksPaginationQuery_graphql,
        identifierInfo: {
          identifierField: "id",
          identifierQueryVariableName: "id",
        },
      },
    },
    name: "readerSeriesWorks",
    selections: [
      {
        alias: "works",
        args: null,
        concreteType: "WorkConnection",
        kind: "LinkedField",
        name: "__readerSeriesWorks_works_connection",
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
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "title",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: [
                      {
                        kind: "Literal",
                        name: "first",
                        value: 5,
                      },
                    ],
                    concreteType: "AuthorConnection",
                    kind: "LinkedField",
                    name: "authors",
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
                              {
                                alias: null,
                                args: null,
                                kind: "ScalarField",
                                name: "name",
                                storageKey: null,
                              },
                            ],
                            storageKey: null,
                          },
                        ],
                        storageKey: null,
                      },
                      v4 /*:: as any*/,
                    ],
                    storageKey: "authors(first:5)",
                  },
                  {
                    alias: null,
                    args: [
                      {
                        kind: "Literal",
                        name: "first",
                        value: 1,
                      },
                    ],
                    concreteType: "ManifestationConnection",
                    kind: "LinkedField",
                    name: "manifestations",
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
                                name: "coverUrl",
                                storageKey: null,
                              },
                            ],
                            storageKey: null,
                          },
                        ],
                        storageKey: null,
                      },
                      v4 /*:: as any*/,
                    ],
                    storageKey: "manifestations(first:1)",
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
            selections: [v3 /*:: as any*/, v2 /*:: as any*/],
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

(node as any).hash = "4216cf6eecd294a1b76fc7812080df14";

export default node;

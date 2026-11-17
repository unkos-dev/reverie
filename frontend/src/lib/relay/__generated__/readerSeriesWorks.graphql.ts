/**
 * @generated SignedSource<<c1a592b3b64791852b911b32e68115e8>>
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
        readonly " $fragmentSpreads": FragmentRefs<"readerWorkAuthors">;
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
        fragmentPathInResult: ["node"],
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
                    args: null,
                    kind: "FragmentSpread",
                    name: "readerWorkAuthors",
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
                      {
                        alias: null,
                        args: null,
                        concreteType: "PageInfo",
                        kind: "LinkedField",
                        name: "pageInfo",
                        plural: false,
                        selections: [v2 /*:: as any*/, v3 /*:: as any*/],
                        storageKey: null,
                      },
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

(node as any).hash = "4d179c54dbd9a042a60b0fb727184ab9";

export default node;

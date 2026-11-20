/**
 * @generated SignedSource<<e97a0070a3d574d11ab4e67532d72de2>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerAuthorEditing$data = {
  readonly id: string;
  readonly metadata: {
    readonly authors: {
      readonly edges: ReadonlyArray<{
        readonly cursor: string;
        readonly node: {
          readonly author: {
            readonly id: string;
          };
          readonly name: string;
        };
      }>;
      readonly pageInfo: {
        readonly endCursor: string | null | undefined;
        readonly hasNextPage: boolean;
        readonly hasPreviousPage: boolean;
        readonly startCursor: string | null | undefined;
      };
    };
    readonly etag: string;
    readonly manifestationId: string;
    readonly workId: string;
  };
  readonly " $fragmentType": "readerAuthorEditing";
};
export type readerAuthorEditing$key = {
  readonly " $data"?: readerAuthorEditing$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerAuthorEditing">;
};

import readerAuthorEditingPaginationQuery_graphql from "./readerAuthorEditingPaginationQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = ["metadata", "authors"],
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
        operation: readerAuthorEditingPaginationQuery_graphql,
        identifierInfo: {
          identifierField: "id",
          identifierQueryVariableName: "id",
        },
      },
    },
    name: "readerAuthorEditing",
    selections: [
      v1 /*:: as any*/,
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
            name: "workId",
            storageKey: null,
          },
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "manifestationId",
            storageKey: null,
          },
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "etag",
            storageKey: null,
          },
          {
            alias: "authors",
            args: null,
            concreteType: "RevisionContributorConnection",
            kind: "LinkedField",
            name: "__readerAuthorEditing_authors_connection",
            plural: false,
            selections: [
              {
                alias: null,
                args: null,
                concreteType: "RevisionContributorEdge",
                kind: "LinkedField",
                name: "edges",
                plural: true,
                selections: [
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "cursor",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    concreteType: "RevisionContributor",
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
                      {
                        alias: null,
                        args: null,
                        concreteType: "Author",
                        kind: "LinkedField",
                        name: "author",
                        plural: false,
                        selections: [v1 /*:: as any*/],
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
                    name: "hasNextPage",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "hasPreviousPage",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "startCursor",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "endCursor",
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
            ],
            storageKey: null,
          },
        ],
        storageKey: null,
      },
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "6640b823801bbb9fa2b7455b9cb9518d";

export default node;

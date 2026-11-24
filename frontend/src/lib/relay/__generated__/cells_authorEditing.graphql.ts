/**
 * @generated SignedSource<<ca518235aa0744c93eee3b3e470277f2>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type cells_authorEditing$data = {
  readonly id: string;
  readonly metadata: {
    readonly authors: {
      readonly edges: ReadonlyArray<{
        readonly node: {
          readonly name: string;
        };
      }>;
    };
    readonly etag: string;
  };
  readonly " $fragmentType": "cells_authorEditing";
};
export type cells_authorEditing$key = {
  readonly " $data"?: cells_authorEditing$data;
  readonly " $fragmentSpreads": FragmentRefs<"cells_authorEditing">;
};

import cellsAuthorEditingPaginationQuery_graphql from "./cellsAuthorEditingPaginationQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = ["metadata", "authors"];
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
        operation: cellsAuthorEditingPaginationQuery_graphql,
        identifierInfo: {
          identifierField: "id",
          identifierQueryVariableName: "id",
        },
      },
      throwOnFieldError: true,
    },
    name: "cells_authorEditing",
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
            name: "etag",
            storageKey: null,
          },
          {
            alias: "authors",
            args: null,
            concreteType: "RevisionContributorConnection",
            kind: "LinkedField",
            name: "__cells_authorEditing_authors_connection",
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
        storageKey: null,
      },
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "02945ce6e526a82df0970e5510599586";

export default node;

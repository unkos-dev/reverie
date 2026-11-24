/**
 * @generated SignedSource<<81f2c57df05e629b839d9051257c262b>>
 * @relayHash 7e18f5a0db19db4090dbac7ed0035a44
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID b460222785ae6a354da9dc1c13b9624200a994516c6a841e884254e8454c1e83

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type VersionsTabPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type VersionsTabPaginationQuery$data = {
  readonly fetch__Manifestation:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"VersionsTab_proposals">;
      }
    | null
    | undefined;
};
export type VersionsTabPaginationQuery = {
  response: VersionsTabPaginationQuery$data;
  variables: VersionsTabPaginationQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
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
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "id",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
    ],
    v2 = [
      {
        kind: "Variable",
        name: "after",
        variableName: "cursor",
      },
      {
        kind: "Variable",
        name: "first",
        variableName: "count",
      },
    ],
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "VersionsTabPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "fetch__Manifestation",
          plural: false,
          selections: [
            {
              args: [
                {
                  kind: "Variable",
                  name: "count",
                  variableName: "count",
                },
                {
                  kind: "Variable",
                  name: "cursor",
                  variableName: "cursor",
                },
              ],
              kind: "FragmentSpread",
              name: "VersionsTab_proposals",
            },
          ],
          storageKey: null,
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "VersionsTabPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "fetch__Manifestation",
          plural: false,
          selections: [
            {
              alias: null,
              args: null,
              concreteType: "MetadataVersions",
              kind: "LinkedField",
              name: "versions",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "pendingCount",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: v2 /*:: as any*/,
                  concreteType: "MetadataVersionConnection",
                  kind: "LinkedField",
                  name: "pending",
                  plural: false,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      concreteType: "MetadataVersionEdge",
                      kind: "LinkedField",
                      name: "edges",
                      plural: true,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          concreteType: "MetadataVersion",
                          kind: "LinkedField",
                          name: "node",
                          plural: false,
                          selections: [
                            v3 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "fieldName",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "source",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "matchType",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "confidenceScore",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              concreteType: null,
                              kind: "LinkedField",
                              name: "newValue",
                              plural: false,
                              selections: [
                                v4 /*:: as any*/,
                                {
                                  kind: "InlineFragment",
                                  selections: [
                                    {
                                      alias: null,
                                      args: null,
                                      kind: "ScalarField",
                                      name: "text",
                                      storageKey: null,
                                    },
                                  ],
                                  type: "MetadataProposalText",
                                  abstractKey: null,
                                },
                                {
                                  kind: "InlineFragment",
                                  selections: [
                                    {
                                      alias: null,
                                      args: null,
                                      kind: "ScalarField",
                                      name: "integer",
                                      storageKey: null,
                                    },
                                  ],
                                  type: "MetadataProposalInteger",
                                  abstractKey: null,
                                },
                                {
                                  kind: "InlineFragment",
                                  selections: [
                                    {
                                      alias: null,
                                      args: null,
                                      kind: "ScalarField",
                                      name: "strings",
                                      storageKey: null,
                                    },
                                  ],
                                  type: "MetadataProposalStringList",
                                  abstractKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                            v4 /*:: as any*/,
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
                {
                  alias: null,
                  args: v2 /*:: as any*/,
                  filters: null,
                  handle: "connection",
                  key: "VersionsTab_proposals_pending",
                  kind: "LinkedHandle",
                  name: "pending",
                },
              ],
              storageKey: null,
            },
            v3 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "b460222785ae6a354da9dc1c13b9624200a994516c6a841e884254e8454c1e83",
      metadata: {},
      name: "VersionsTabPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "a6fd72d7158a56128ec5d3f3f0f06137";

export default node;

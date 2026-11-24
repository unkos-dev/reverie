/**
 * @generated SignedSource<<44c42a6245a269940ec5b069c22bda76>>
 * @relayHash 51181df2adbeefed13f50b4f9c1968b3
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 31651cf99ac6662818d9636d5a74ccf47db12fcf3096eaf4806eab1984dfbac4

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type VersionsTabQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type VersionsTabQuery$data = {
  readonly manifestation:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"VersionsTab_proposals">;
      }
    | null
    | undefined;
};
export type VersionsTabQuery = {
  response: VersionsTabQuery$data;
  variables: VersionsTabQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: 20,
      kind: "LocalArgument",
      name: "count",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "cursor",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v3 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
    ],
    v4 = [
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
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/],
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "VersionsTabQuery",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
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
      argumentDefinitions: [v2 /*:: as any*/, v0 /*:: as any*/, v1 /*:: as any*/],
      kind: "Operation",
      name: "VersionsTabQuery",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
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
                  args: v4 /*:: as any*/,
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
                            v5 /*:: as any*/,
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
                                v6 /*:: as any*/,
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
                                {
                                  kind: "InlineFragment",
                                  selections: [
                                    v7 /*:: as any*/,
                                    {
                                      alias: null,
                                      args: null,
                                      kind: "ScalarField",
                                      name: "position",
                                      storageKey: null,
                                    },
                                  ],
                                  type: "MetadataProposalSeries",
                                  abstractKey: null,
                                },
                                {
                                  kind: "InlineFragment",
                                  selections: [
                                    {
                                      alias: null,
                                      args: null,
                                      concreteType: "MetadataProposalContributor",
                                      kind: "LinkedField",
                                      name: "contributors",
                                      plural: true,
                                      selections: [
                                        v7 /*:: as any*/,
                                        {
                                          alias: null,
                                          args: null,
                                          kind: "ScalarField",
                                          name: "code",
                                          storageKey: null,
                                        },
                                      ],
                                      storageKey: null,
                                    },
                                  ],
                                  type: "MetadataProposalContributors",
                                  abstractKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                            v6 /*:: as any*/,
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
                  args: v4 /*:: as any*/,
                  filters: null,
                  handle: "connection",
                  key: "VersionsTab_proposals_pending",
                  kind: "LinkedHandle",
                  name: "pending",
                },
              ],
              storageKey: null,
            },
            v5 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "31651cf99ac6662818d9636d5a74ccf47db12fcf3096eaf4806eab1984dfbac4",
      metadata: {},
      name: "VersionsTabQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "94b8fd550ecf086b379a75ee1cc60c12";

export default node;

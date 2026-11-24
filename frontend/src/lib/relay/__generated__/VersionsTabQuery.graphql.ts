/**
 * @generated SignedSource<<fe366d557c8e65155f203a9a457a7c5a>>
 * @relayHash e4f9d40061254e87689f387f5e27e92c
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID f94764a59753b7250a07bf83a8ae5a5fa0ccde1bb5159f1ee08594c494874489

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
        readonly work:
          | {
              readonly id: string;
            }
          | null
          | undefined;
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
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      concreteType: "Work",
      kind: "LinkedField",
      name: "work",
      plural: false,
      selections: [v4 /*:: as any*/],
      storageKey: null,
    },
    v6 = [
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
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v8 = {
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
            v5 /*:: as any*/,
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
            v5 /*:: as any*/,
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
                  args: v6 /*:: as any*/,
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
                            v4 /*:: as any*/,
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
                                v7 /*:: as any*/,
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
                                    v8 /*:: as any*/,
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
                                        v8 /*:: as any*/,
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
                            v7 /*:: as any*/,
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
                  args: v6 /*:: as any*/,
                  filters: null,
                  handle: "connection",
                  key: "VersionsTab_proposals_pending",
                  kind: "LinkedHandle",
                  name: "pending",
                },
                {
                  kind: "ClientExtension",
                  selections: [
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "__id",
                      storageKey: null,
                    },
                  ],
                },
              ],
              storageKey: null,
            },
            v4 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "f94764a59753b7250a07bf83a8ae5a5fa0ccde1bb5159f1ee08594c494874489",
      metadata: {},
      name: "VersionsTabQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "21a9dceac2e07a75613d710864499658";

export default node;

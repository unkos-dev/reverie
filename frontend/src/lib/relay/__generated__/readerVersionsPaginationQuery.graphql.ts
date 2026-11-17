/**
 * @generated SignedSource<<5a021dc35357e68a27a2218c0a864826>>
 * @relayHash 3043a35c0a5261ddbed8ad4eaea39f1d
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID c4356b4727ea448d6d58321baa26f0b6d1e69f1d64631db4b821d84139f9e72c

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerVersionsPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type readerVersionsPaginationQuery$data = {
  readonly node:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerVersions">;
      }
    | null
    | undefined;
};
export type readerVersionsPaginationQuery = {
  response: readerVersionsPaginationQuery$data;
  variables: readerVersionsPaginationQuery$variables;
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
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
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
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerVersionsPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "node",
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
              name: "readerVersions",
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
      name: "readerVersionsPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "node",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            v3 /*:: as any*/,
            {
              kind: "InlineFragment",
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
                                  name: "newValue",
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
                                  kind: "ScalarField",
                                  name: "matchType",
                                  storageKey: null,
                                },
                                {
                                  alias: null,
                                  args: null,
                                  kind: "ScalarField",
                                  name: "status",
                                  storageKey: null,
                                },
                                {
                                  alias: null,
                                  args: null,
                                  kind: "ScalarField",
                                  name: "observationCount",
                                  storageKey: null,
                                },
                                v2 /*:: as any*/,
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
                      key: "readerVersions_pending",
                      kind: "LinkedHandle",
                      name: "pending",
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "Manifestation",
              abstractKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "c4356b4727ea448d6d58321baa26f0b6d1e69f1d64631db4b821d84139f9e72c",
      metadata: {},
      name: "readerVersionsPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "7e69bbe20c4e178072f0cf2cba45f207";

export default node;

/**
 * @generated SignedSource<<85e1880c482c0ec7aae52988acfb7776>>
 * @relayHash 8603558ea66a5ffa4246a37319623350
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 3cebcc98a7a8612d9f860f237850e8cd58bee8ec07fab4e42dae6ac6854d9944

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type cellsAuthorsQuery$variables = {
  id: string;
};
export type cellsAuthorsQuery$data = {
  readonly manifestation:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerAuthorEditing">;
      }
    | null
    | undefined;
};
export type cellsAuthorsQuery = {
  response: cellsAuthorsQuery$data;
  variables: cellsAuthorsQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
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
      name: "id",
      storageKey: null,
    },
    v3 = [
      {
        kind: "Literal",
        name: "first",
        value: 20,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "cellsAuthorsQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            {
              args: null,
              kind: "FragmentSpread",
              name: "readerAuthorEditing",
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
      name: "cellsAuthorsQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
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
                  alias: null,
                  args: v3 /*:: as any*/,
                  concreteType: "RevisionContributorConnection",
                  kind: "LinkedField",
                  name: "authors",
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
                  storageKey: "authors(first:20)",
                },
                {
                  alias: null,
                  args: v3 /*:: as any*/,
                  filters: null,
                  handle: "connection",
                  key: "readerAuthorEditing_authors",
                  kind: "LinkedHandle",
                  name: "authors",
                },
                v2 /*:: as any*/,
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "3cebcc98a7a8612d9f860f237850e8cd58bee8ec07fab4e42dae6ac6854d9944",
      metadata: {},
      name: "cellsAuthorsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "86bf79db2595220a0df0ab9e5a2c0d7d";

export default node;

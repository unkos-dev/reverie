/**
 * @generated SignedSource<<3c6ff213152a77a7d8392faf87882a13>>
 * @relayHash fdba39453e1d9e19e9746c9a65defbe7
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 15a6623a11a326279a73b4f9698074b0c65fa80891b6ad35beb48f60461413e8

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type CommandPaletteQuery$variables = {
  q: string;
};
export type CommandPaletteQuery$data = {
  readonly " $fragmentSpreads": FragmentRefs<"CommandPalette_results">;
};
export type CommandPaletteQuery = {
  response: CommandPaletteQuery$data;
  variables: CommandPaletteQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "q",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "q",
        variableName: "q",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "CommandPaletteQuery",
      selections: [
        {
          args: v1 /*:: as any*/,
          kind: "FragmentSpread",
          name: "CommandPalette_results",
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "CommandPaletteQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "SearchHit",
          kind: "LinkedField",
          name: "search",
          plural: true,
          selections: [
            {
              alias: null,
              args: null,
              concreteType: "Manifestation",
              kind: "LinkedField",
              name: "manifestation",
              plural: false,
              selections: [
                v2 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Work",
                  kind: "LinkedField",
                  name: "work",
                  plural: false,
                  selections: [
                    v2 /*:: as any*/,
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
                                v2 /*:: as any*/,
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
                              name: "endCursor",
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                      ],
                      storageKey: "authors(first:5)",
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
              kind: "ScalarField",
              name: "snippet",
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "15a6623a11a326279a73b4f9698074b0c65fa80891b6ad35beb48f60461413e8",
      metadata: {},
      name: "CommandPaletteQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "e5b614de6cec9524c28ba1eb8bf636ed";

export default node;

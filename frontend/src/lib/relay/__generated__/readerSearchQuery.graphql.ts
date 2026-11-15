/**
 * @generated SignedSource<<86bb72f1c59a1932493c392cb8e10993>>
 * @relayHash 6d3ebd234a40d2a21a5598c4630ab644
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID cdcdd13460512dd0f684337bfcaa9555a1008f043da93b8428873ee36601c907

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSearchQuery$variables = {
  q: string;
};
export type readerSearchQuery$data = {
  readonly search: ReadonlyArray<{
    readonly manifestation:
      | {
          readonly " $fragmentSpreads": FragmentRefs<"readerSearchHit">;
        }
      | null
      | undefined;
    readonly snippet: string | null | undefined;
  }>;
};
export type readerSearchQuery = {
  response: readerSearchQuery$data;
  variables: readerSearchQuery$variables;
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
      name: "snippet",
      storageKey: null,
    },
    v3 = {
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
      metadata: null,
      name: "readerSearchQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "SearchHit",
          kind: "LinkedField",
          name: "search",
          plural: true,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "Manifestation",
              kind: "LinkedField",
              name: "manifestation",
              plural: false,
              selections: [
                {
                  args: null,
                  kind: "FragmentSpread",
                  name: "readerSearchHit",
                },
              ],
              storageKey: null,
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
      name: "readerSearchQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "SearchHit",
          kind: "LinkedField",
          name: "search",
          plural: true,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "Manifestation",
              kind: "LinkedField",
              name: "manifestation",
              plural: false,
              selections: [
                v3 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Work",
                  kind: "LinkedField",
                  name: "work",
                  plural: false,
                  selections: [
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
                                v3 /*:: as any*/,
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
                    v3 /*:: as any*/,
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
    },
    params: {
      id: "cdcdd13460512dd0f684337bfcaa9555a1008f043da93b8428873ee36601c907",
      metadata: {},
      name: "readerSearchQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "b8a0ad2efeb763e7b2977aa116a8cae2";

export default node;

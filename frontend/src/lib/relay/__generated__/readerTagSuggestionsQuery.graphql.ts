/**
 * @generated SignedSource<<5dbff2dbea234d62c1e13e6d6e5a1215>>
 * @relayHash 2217f9be411037b92c9d0c5313bd07de
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID eec87e0689037547d6f2e7cec5f209f2e41040d94daa244463758cc640df6093

import { ConcreteRequest } from "relay-runtime";
export type readerTagSuggestionsQuery$variables = {
  q: string;
};
export type readerTagSuggestionsQuery$data = {
  readonly tagSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
};
export type readerTagSuggestionsQuery = {
  response: readerTagSuggestionsQuery$data;
  variables: readerTagSuggestionsQuery$variables;
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
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "q",
            variableName: "q",
          },
        ],
        concreteType: "TagSuggestion",
        kind: "LinkedField",
        name: "tagSuggestions",
        plural: true,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "entityId",
            storageKey: null,
          },
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "value",
            storageKey: null,
          },
        ],
        storageKey: null,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerTagSuggestionsQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerTagSuggestionsQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "eec87e0689037547d6f2e7cec5f209f2e41040d94daa244463758cc640df6093",
      metadata: {},
      name: "readerTagSuggestionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "6812ebc1f328bf7db1fa0af28601f594";

export default node;

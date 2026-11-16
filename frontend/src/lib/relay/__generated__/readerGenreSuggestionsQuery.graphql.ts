/**
 * @generated SignedSource<<72b1b32b7ec87094878cdfe5104983c6>>
 * @relayHash 920ab26011e9112ef7d9993b19aa9644
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 47f8d5a8fd1a4c842016f8993b08ae8efafff4c0690ea25f6a2b12ab0352e886

import { ConcreteRequest } from "relay-runtime";
export type readerGenreSuggestionsQuery$variables = {
  q: string;
};
export type readerGenreSuggestionsQuery$data = {
  readonly genreSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
};
export type readerGenreSuggestionsQuery = {
  response: readerGenreSuggestionsQuery$data;
  variables: readerGenreSuggestionsQuery$variables;
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
        concreteType: "GenreSuggestion",
        kind: "LinkedField",
        name: "genreSuggestions",
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
      name: "readerGenreSuggestionsQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerGenreSuggestionsQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "47f8d5a8fd1a4c842016f8993b08ae8efafff4c0690ea25f6a2b12ab0352e886",
      metadata: {},
      name: "readerGenreSuggestionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "fda172786c7b72f8dfe2841b4ab51453";

export default node;

/**
 * @generated SignedSource<<53bd1bd92e1d3852ad9d66ffd2619c5e>>
 * @relayHash 2ce307d7fc4e9753cea1dd3bebf1d331
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 51b1fe0fb4824404cdcfc6960361ccf96f0b088ef048ba0c97bcdd5d7f1cd002

import { ConcreteRequest } from "relay-runtime";
export type readerAuthorSuggestionsQuery$variables = {
  q: string;
};
export type readerAuthorSuggestionsQuery$data = {
  readonly authorSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
};
export type readerAuthorSuggestionsQuery = {
  response: readerAuthorSuggestionsQuery$data;
  variables: readerAuthorSuggestionsQuery$variables;
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
        concreteType: "AuthorSuggestion",
        kind: "LinkedField",
        name: "authorSuggestions",
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
      name: "readerAuthorSuggestionsQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerAuthorSuggestionsQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "51b1fe0fb4824404cdcfc6960361ccf96f0b088ef048ba0c97bcdd5d7f1cd002",
      metadata: {},
      name: "readerAuthorSuggestionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "f8b5ac0a35d653620d3c1b2c0b320d8f";

export default node;

/**
 * @generated SignedSource<<9a8db91563834db85a9c78d0c7524a9d>>
 * @relayHash a6ad83eb2144e0f38b88812cf53c7716
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 9e28178f7d5972474da4301dd178a1dafba38d2b537fdfbced895f0390c96bac

import { ConcreteRequest } from "relay-runtime";
export type readerSeriesSuggestionsQuery$variables = {
  q: string;
};
export type readerSeriesSuggestionsQuery$data = {
  readonly seriesSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
};
export type readerSeriesSuggestionsQuery = {
  response: readerSeriesSuggestionsQuery$data;
  variables: readerSeriesSuggestionsQuery$variables;
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
        concreteType: "SeriesSuggestion",
        kind: "LinkedField",
        name: "seriesSuggestions",
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
      name: "readerSeriesSuggestionsQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerSeriesSuggestionsQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "9e28178f7d5972474da4301dd178a1dafba38d2b537fdfbced895f0390c96bac",
      metadata: {},
      name: "readerSeriesSuggestionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "bb5a0fcde0232894cb939a0465cd6c94";

export default node;

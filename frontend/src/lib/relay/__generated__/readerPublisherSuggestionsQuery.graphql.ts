/**
 * @generated SignedSource<<baf48fc58d45d4fd629ac561e233fad8>>
 * @relayHash 3bc7a4af53843c8f9f794c42ae0c2480
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 57f462385acea998d4c08d4149fdfc96882e3036076f67681a1b1ae5673b3063

import { ConcreteRequest } from "relay-runtime";
export type readerPublisherSuggestionsQuery$variables = {
  q: string;
};
export type readerPublisherSuggestionsQuery$data = {
  readonly publisherSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
};
export type readerPublisherSuggestionsQuery = {
  response: readerPublisherSuggestionsQuery$data;
  variables: readerPublisherSuggestionsQuery$variables;
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
        concreteType: "PublisherSuggestion",
        kind: "LinkedField",
        name: "publisherSuggestions",
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
      name: "readerPublisherSuggestionsQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerPublisherSuggestionsQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "57f462385acea998d4c08d4149fdfc96882e3036076f67681a1b1ae5673b3063",
      metadata: {},
      name: "readerPublisherSuggestionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "9ecedab73343503154212bcb65f9384c";

export default node;

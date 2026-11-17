/**
 * @generated SignedSource<<6f3943150b2e189868bfa01570ed217b>>
 * @relayHash 39d3fd74dcdc2966f786df2561a4d5b9
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 69698b616ca457fd83670dfbe9acd17c6d8b12a9092168492fab7f9af7afe6dd

import { ConcreteRequest } from "relay-runtime";
export type readerMoodSuggestionsQuery$variables = {
  q: string;
};
export type readerMoodSuggestionsQuery$data = {
  readonly moodSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
};
export type readerMoodSuggestionsQuery = {
  response: readerMoodSuggestionsQuery$data;
  variables: readerMoodSuggestionsQuery$variables;
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
        concreteType: "MoodSuggestion",
        kind: "LinkedField",
        name: "moodSuggestions",
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
      name: "readerMoodSuggestionsQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerMoodSuggestionsQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "69698b616ca457fd83670dfbe9acd17c6d8b12a9092168492fab7f9af7afe6dd",
      metadata: {},
      name: "readerMoodSuggestionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "701072c52fbc20908da61a30089bdc20";

export default node;

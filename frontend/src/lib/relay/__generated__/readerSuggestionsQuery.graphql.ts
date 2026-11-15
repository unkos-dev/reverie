/**
 * @generated SignedSource<<bb6c94e7181383efec23b2b6976cf4a5>>
 * @relayHash c19651cb6744cc1919980c60f4bbc937
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID d1ec69c31220fd6e48a3439ff99b5fd8b9b5247ca50a8175ac266c00c9acf1a9

import { ConcreteRequest } from "relay-runtime";
export type readerSuggestionsQuery$variables = {
  authorIds: ReadonlyArray<string>;
  q: string;
  seriesIds: ReadonlyArray<string>;
};
export type readerSuggestionsQuery$data = {
  readonly authorLabels: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
  readonly authorSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
  readonly genreSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
  readonly moodSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
  readonly publisherSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
  readonly seriesLabels: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
  readonly seriesSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
  readonly tagSuggestions: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
};
export type readerSuggestionsQuery = {
  response: readerSuggestionsQuery$data;
  variables: readerSuggestionsQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "authorIds",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "q",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "seriesIds",
    },
    v3 = [
      {
        kind: "Variable",
        name: "q",
        variableName: "q",
      },
    ],
    v4 = [
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
    v5 = [
      {
        alias: null,
        args: v3 /*:: as any*/,
        concreteType: "AuthorSuggestion",
        kind: "LinkedField",
        name: "authorSuggestions",
        plural: true,
        selections: v4 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: v3 /*:: as any*/,
        concreteType: "SeriesSuggestion",
        kind: "LinkedField",
        name: "seriesSuggestions",
        plural: true,
        selections: v4 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: v3 /*:: as any*/,
        concreteType: "GenreSuggestion",
        kind: "LinkedField",
        name: "genreSuggestions",
        plural: true,
        selections: v4 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: v3 /*:: as any*/,
        concreteType: "MoodSuggestion",
        kind: "LinkedField",
        name: "moodSuggestions",
        plural: true,
        selections: v4 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: v3 /*:: as any*/,
        concreteType: "TagSuggestion",
        kind: "LinkedField",
        name: "tagSuggestions",
        plural: true,
        selections: v4 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: v3 /*:: as any*/,
        concreteType: "PublisherSuggestion",
        kind: "LinkedField",
        name: "publisherSuggestions",
        plural: true,
        selections: v4 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "ids",
            variableName: "authorIds",
          },
        ],
        concreteType: "AuthorSuggestion",
        kind: "LinkedField",
        name: "authorLabels",
        plural: true,
        selections: v4 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "ids",
            variableName: "seriesIds",
          },
        ],
        concreteType: "SeriesSuggestion",
        kind: "LinkedField",
        name: "seriesLabels",
        plural: true,
        selections: v4 /*:: as any*/,
        storageKey: null,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "readerSuggestionsQuery",
      selections: v5 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v2 /*:: as any*/],
      kind: "Operation",
      name: "readerSuggestionsQuery",
      selections: v5 /*:: as any*/,
    },
    params: {
      id: "d1ec69c31220fd6e48a3439ff99b5fd8b9b5247ca50a8175ac266c00c9acf1a9",
      metadata: {},
      name: "readerSuggestionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "bee705358057d02d9e8c9679586f8297";

export default node;

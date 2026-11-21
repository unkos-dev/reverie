/**
 * @generated SignedSource<<df65aceeed15d04e9af8d256f3e7bb05>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type TypeaheadMultiSelect_suggestions$data = {
  readonly authorSuggestions?: ReadonlyArray<{
    readonly id: string;
    readonly name: string;
  }>;
  readonly genreSuggestions?: ReadonlyArray<{
    readonly id: string;
    readonly name: string;
  }>;
  readonly moodSuggestions?: ReadonlyArray<{
    readonly id: string;
    readonly name: string;
  }>;
  readonly seriesSuggestions?: ReadonlyArray<{
    readonly id: string;
    readonly name: string;
  }>;
  readonly tagSuggestions?: ReadonlyArray<{
    readonly id: string;
    readonly name: string;
  }>;
  readonly " $fragmentType": "TypeaheadMultiSelect_suggestions";
};
export type TypeaheadMultiSelect_suggestions$key = {
  readonly " $data"?: TypeaheadMultiSelect_suggestions$data;
  readonly " $fragmentSpreads": FragmentRefs<"TypeaheadMultiSelect_suggestions">;
};

import TypeaheadMultiSelectRefetchQuery_graphql from "./TypeaheadMultiSelectRefetchQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = [
      {
        kind: "Variable",
        name: "q",
        variableName: "q",
      },
    ],
    v1 = [
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "id",
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "name",
        storageKey: null,
      },
    ];
  return {
    argumentDefinitions: [
      {
        defaultValue: false,
        kind: "LocalArgument",
        name: "authors",
      },
      {
        defaultValue: false,
        kind: "LocalArgument",
        name: "genres",
      },
      {
        defaultValue: false,
        kind: "LocalArgument",
        name: "moods",
      },
      {
        defaultValue: "",
        kind: "LocalArgument",
        name: "q",
      },
      {
        defaultValue: false,
        kind: "LocalArgument",
        name: "series",
      },
      {
        defaultValue: false,
        kind: "LocalArgument",
        name: "tags",
      },
    ],
    kind: "Fragment",
    metadata: {
      refetch: {
        connection: null,
        fragmentPathInResult: [],
        operation: TypeaheadMultiSelectRefetchQuery_graphql,
      },
      throwOnFieldError: true,
    },
    name: "TypeaheadMultiSelect_suggestions",
    selections: [
      {
        condition: "authors",
        kind: "Condition",
        passingValue: true,
        selections: [
          {
            alias: null,
            args: v0 /*:: as any*/,
            concreteType: "Author",
            kind: "LinkedField",
            name: "authorSuggestions",
            plural: true,
            selections: v1 /*:: as any*/,
            storageKey: null,
          },
        ],
      },
      {
        condition: "series",
        kind: "Condition",
        passingValue: true,
        selections: [
          {
            alias: null,
            args: v0 /*:: as any*/,
            concreteType: "Series",
            kind: "LinkedField",
            name: "seriesSuggestions",
            plural: true,
            selections: v1 /*:: as any*/,
            storageKey: null,
          },
        ],
      },
      {
        condition: "genres",
        kind: "Condition",
        passingValue: true,
        selections: [
          {
            alias: null,
            args: v0 /*:: as any*/,
            concreteType: "Genre",
            kind: "LinkedField",
            name: "genreSuggestions",
            plural: true,
            selections: v1 /*:: as any*/,
            storageKey: null,
          },
        ],
      },
      {
        condition: "moods",
        kind: "Condition",
        passingValue: true,
        selections: [
          {
            alias: null,
            args: v0 /*:: as any*/,
            concreteType: "Mood",
            kind: "LinkedField",
            name: "moodSuggestions",
            plural: true,
            selections: v1 /*:: as any*/,
            storageKey: null,
          },
        ],
      },
      {
        condition: "tags",
        kind: "Condition",
        passingValue: true,
        selections: [
          {
            alias: null,
            args: v0 /*:: as any*/,
            concreteType: "Tag",
            kind: "LinkedField",
            name: "tagSuggestions",
            plural: true,
            selections: v1 /*:: as any*/,
            storageKey: null,
          },
        ],
      },
    ],
    type: "Query",
    abstractKey: null,
  };
})();

(node as any).hash = "11be2139830265961c04f6e24948d079";

export default node;

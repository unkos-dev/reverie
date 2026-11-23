/**
 * @generated SignedSource<<d4d6ac34f0746e0e2caf47a46854e247>>
 * @relayHash 1a525b85541218527c1b965ed7fa6e75
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID a580bce4a1273768301b69edb623764f3343def16290e9153a947e8b29cec4c9

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type TypeaheadMultiSelectRefetchQuery$variables = {
  authors?: boolean | null | undefined;
  genres?: boolean | null | undefined;
  moods?: boolean | null | undefined;
  q?: string | null | undefined;
  series?: boolean | null | undefined;
  tags?: boolean | null | undefined;
};
export type TypeaheadMultiSelectRefetchQuery$data = {
  readonly " $fragmentSpreads": FragmentRefs<"TypeaheadMultiSelect_suggestions">;
};
export type TypeaheadMultiSelectRefetchQuery = {
  response: TypeaheadMultiSelectRefetchQuery$data;
  variables: TypeaheadMultiSelectRefetchQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
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
    v1 = {
      kind: "Variable",
      name: "q",
      variableName: "q",
    },
    v2 = [v1 /*:: as any*/],
    v3 = [
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
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "TypeaheadMultiSelectRefetchQuery",
      selections: [
        {
          args: [
            {
              kind: "Variable",
              name: "authors",
              variableName: "authors",
            },
            {
              kind: "Variable",
              name: "genres",
              variableName: "genres",
            },
            {
              kind: "Variable",
              name: "moods",
              variableName: "moods",
            },
            v1 /*:: as any*/,
            {
              kind: "Variable",
              name: "series",
              variableName: "series",
            },
            {
              kind: "Variable",
              name: "tags",
              variableName: "tags",
            },
          ],
          kind: "FragmentSpread",
          name: "TypeaheadMultiSelect_suggestions",
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "TypeaheadMultiSelectRefetchQuery",
      selections: [
        {
          condition: "authors",
          kind: "Condition",
          passingValue: true,
          selections: [
            {
              alias: null,
              args: v2 /*:: as any*/,
              concreteType: "Author",
              kind: "LinkedField",
              name: "authorSuggestions",
              plural: true,
              selections: v3 /*:: as any*/,
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
              args: v2 /*:: as any*/,
              concreteType: "Series",
              kind: "LinkedField",
              name: "seriesSuggestions",
              plural: true,
              selections: v3 /*:: as any*/,
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
              args: v2 /*:: as any*/,
              concreteType: "Genre",
              kind: "LinkedField",
              name: "genreSuggestions",
              plural: true,
              selections: v3 /*:: as any*/,
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
              args: v2 /*:: as any*/,
              concreteType: "Mood",
              kind: "LinkedField",
              name: "moodSuggestions",
              plural: true,
              selections: v3 /*:: as any*/,
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
              args: v2 /*:: as any*/,
              concreteType: "Tag",
              kind: "LinkedField",
              name: "tagSuggestions",
              plural: true,
              selections: v3 /*:: as any*/,
              storageKey: null,
            },
          ],
        },
      ],
    },
    params: {
      id: "a580bce4a1273768301b69edb623764f3343def16290e9153a947e8b29cec4c9",
      metadata: {},
      name: "TypeaheadMultiSelectRefetchQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "11be2139830265961c04f6e24948d079";

export default node;

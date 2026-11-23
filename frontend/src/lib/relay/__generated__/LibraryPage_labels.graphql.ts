/**
 * @generated SignedSource<<d20e4d01138838db166190c75bda642c>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type LibraryPage_labels$data = {
  readonly authorLabels: ReadonlyArray<{
    readonly id: string;
    readonly name: string;
  }>;
  readonly seriesLabels: ReadonlyArray<{
    readonly id: string;
    readonly name: string;
  }>;
  readonly shelfLabels: ReadonlyArray<{
    readonly id: string;
    readonly itemCount: number;
    readonly name: string;
  }>;
  readonly " $fragmentType": "LibraryPage_labels";
};
export type LibraryPage_labels$key = {
  readonly " $data"?: LibraryPage_labels$data;
  readonly " $fragmentSpreads": FragmentRefs<"LibraryPage_labels">;
};

import LibraryPageLabelsRefetchQuery_graphql from "./LibraryPageLabelsRefetchQuery.graphql";

const node: ReaderFragment = (function () {
  var v0 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v2 = [v0 /*:: as any*/, v1 /*:: as any*/];
  return {
    argumentDefinitions: [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "authorIds",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "seriesIds",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "shelfIds",
      },
    ],
    kind: "Fragment",
    metadata: {
      refetch: {
        connection: null,
        fragmentPathInResult: [],
        operation: LibraryPageLabelsRefetchQuery_graphql,
      },
      throwOnFieldError: true,
    },
    name: "LibraryPage_labels",
    selections: [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "ids",
            variableName: "authorIds",
          },
        ],
        concreteType: "Author",
        kind: "LinkedField",
        name: "authorLabels",
        plural: true,
        selections: v2 /*:: as any*/,
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
        concreteType: "Series",
        kind: "LinkedField",
        name: "seriesLabels",
        plural: true,
        selections: v2 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "ids",
            variableName: "shelfIds",
          },
        ],
        concreteType: "Shelf",
        kind: "LinkedField",
        name: "shelfLabels",
        plural: true,
        selections: [
          v0 /*:: as any*/,
          v1 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "itemCount",
            storageKey: null,
          },
        ],
        storageKey: null,
      },
    ],
    type: "Query",
    abstractKey: null,
  };
})();

(node as any).hash = "f4af5e99251eeb172a1ac47db9166dd7";

export default node;

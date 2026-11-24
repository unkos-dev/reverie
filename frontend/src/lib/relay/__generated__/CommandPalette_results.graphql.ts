/**
 * @generated SignedSource<<a7a2395f30cf8f4519168ad581401d2a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type CommandPalette_results$data = {
  readonly search: ReadonlyArray<{
    readonly manifestation:
      | {
          readonly id: string;
          readonly work:
            | {
                readonly id: string;
              }
            | null
            | undefined;
        }
      | null
      | undefined;
    readonly " $fragmentSpreads": FragmentRefs<"CommandPalette_hit">;
  }>;
  readonly " $fragmentType": "CommandPalette_results";
};
export type CommandPalette_results$key = {
  readonly " $data"?: CommandPalette_results$data;
  readonly " $fragmentSpreads": FragmentRefs<"CommandPalette_results">;
};

const node: ReaderFragment = (function () {
  var v0 = {
    alias: null,
    args: null,
    kind: "ScalarField",
    name: "id",
    storageKey: null,
  };
  return {
    argumentDefinitions: [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "q",
      },
    ],
    kind: "Fragment",
    metadata: {
      throwOnFieldError: true,
    },
    name: "CommandPalette_results",
    selections: [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "q",
            variableName: "q",
          },
        ],
        concreteType: "SearchHit",
        kind: "LinkedField",
        name: "search",
        plural: true,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "Manifestation",
            kind: "LinkedField",
            name: "manifestation",
            plural: false,
            selections: [
              v0 /*:: as any*/,
              {
                alias: null,
                args: null,
                concreteType: "Work",
                kind: "LinkedField",
                name: "work",
                plural: false,
                selections: [v0 /*:: as any*/],
                storageKey: null,
              },
            ],
            storageKey: null,
          },
          {
            args: null,
            kind: "FragmentSpread",
            name: "CommandPalette_hit",
          },
        ],
        storageKey: null,
      },
    ],
    type: "Query",
    abstractKey: null,
  };
})();

(node as any).hash = "97bdcf2c9884c20de4285f2ee60829a4";

export default node;

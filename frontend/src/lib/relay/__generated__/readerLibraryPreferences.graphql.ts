/**
 * @generated SignedSource<<ccbf2841bae5246e3af5a875e6b6e95a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerLibraryPreferences$data = {
  readonly defaults: {
    readonly density: string;
    readonly hiddenColumns: ReadonlyArray<string>;
    readonly sortStack: string;
    readonly view: string;
  };
  readonly density: string | null | undefined;
  readonly hiddenColumns: ReadonlyArray<string> | null | undefined;
  readonly sortStack: string | null | undefined;
  readonly view: string | null | undefined;
  readonly " $fragmentType": "readerLibraryPreferences";
};
export type readerLibraryPreferences$key = {
  readonly " $data"?: readerLibraryPreferences$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerLibraryPreferences">;
};

const node: ReaderFragment = (function () {
  var v0 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "sortStack",
      storageKey: null,
    };
  return {
    argumentDefinitions: [],
    kind: "Fragment",
    metadata: null,
    name: "readerLibraryPreferences",
    selections: [
      v0 /*:: as any*/,
      v1 /*:: as any*/,
      v2 /*:: as any*/,
      v3 /*:: as any*/,
      {
        alias: null,
        args: null,
        concreteType: "LibraryPreferenceDefaults",
        kind: "LinkedField",
        name: "defaults",
        plural: false,
        selections: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/, v3 /*:: as any*/],
        storageKey: null,
      },
    ],
    type: "LibraryPreferences",
    abstractKey: null,
  };
})();

(node as any).hash = "3bdeffc0242bfb2e7d7bfd3a4e672c86";

export default node;

/**
 * @generated SignedSource<<d08143f175269b8a59345f225b4f6823>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type mutationsUndoRefusal$data = {
  readonly fields?: ReadonlyArray<string>;
  readonly message?: string;
  readonly " $fragmentType": "mutationsUndoRefusal";
};
export type mutationsUndoRefusal$key = {
  readonly " $data"?: mutationsUndoRefusal$data;
  readonly " $fragmentSpreads": FragmentRefs<"mutationsUndoRefusal">;
};

const node: ReaderFragment = (function () {
  var v0 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "message",
      storageKey: null,
    },
    v1 = [v0 /*:: as any*/];
  return {
    argumentDefinitions: [],
    kind: "Fragment",
    metadata: null,
    name: "mutationsUndoRefusal",
    selections: [
      {
        kind: "InlineFragment",
        selections: v1 /*:: as any*/,
        type: "StaleBaseline",
        abstractKey: null,
      },
      {
        kind: "InlineFragment",
        selections: v1 /*:: as any*/,
        type: "UndoUnavailable",
        abstractKey: null,
      },
      {
        kind: "InlineFragment",
        selections: v1 /*:: as any*/,
        type: "UndoConflict",
        abstractKey: null,
      },
      {
        kind: "InlineFragment",
        selections: [
          v0 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "fields",
            storageKey: null,
          },
        ],
        type: "FieldsLocked",
        abstractKey: null,
      },
      {
        kind: "InlineFragment",
        selections: v1 /*:: as any*/,
        type: "EditTooLarge",
        abstractKey: null,
      },
      {
        kind: "InlineFragment",
        selections: v1 /*:: as any*/,
        type: "NotFound",
        abstractKey: null,
      },
    ],
    type: "UndoMetadataResult",
    abstractKey: "__isUndoMetadataResult",
  };
})();

(node as any).hash = "a555e0de5db65438499718051645fafe";

export default node;

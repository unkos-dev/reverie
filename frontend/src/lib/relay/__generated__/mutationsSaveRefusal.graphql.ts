/**
 * @generated SignedSource<<f99fc40430405b6f76b747733e37e550>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type mutationsSaveRefusal$data = {
  readonly fieldErrors?: ReadonlyArray<{
    readonly field: string;
    readonly message: string;
  }>;
  readonly fields?: ReadonlyArray<string>;
  readonly message?: string;
  readonly " $fragmentType": "mutationsSaveRefusal";
};
export type mutationsSaveRefusal$key = {
  readonly " $data"?: mutationsSaveRefusal$data;
  readonly " $fragmentSpreads": FragmentRefs<"mutationsSaveRefusal">;
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
    name: "mutationsSaveRefusal",
    selections: [
      {
        kind: "InlineFragment",
        selections: v1 /*:: as any*/,
        type: "StaleBaseline",
        abstractKey: null,
      },
      {
        kind: "InlineFragment",
        selections: [
          v0 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "FieldError",
            kind: "LinkedField",
            name: "fieldErrors",
            plural: true,
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "field",
                storageKey: null,
              },
              v0 /*:: as any*/,
            ],
            storageKey: null,
          },
        ],
        type: "MetadataInvalid",
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
        type: "NotFound",
        abstractKey: null,
      },
    ],
    type: "UpdateMetadataResult",
    abstractKey: "__isUpdateMetadataResult",
  };
})();

(node as any).hash = "9c9e354fb934f8cd1a183057535eb171";

export default node;

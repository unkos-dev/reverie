/**
 * @generated SignedSource<<e66aa114a6b9dc61857c7b746fffd204>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ShelfHeader_shelves$data = ReadonlyArray<{
  readonly id: string;
  readonly name: string;
  readonly " $fragmentType": "ShelfHeader_shelves";
}>;
export type ShelfHeader_shelves$key = ReadonlyArray<{
  readonly " $data"?: ShelfHeader_shelves$data;
  readonly " $fragmentSpreads": FragmentRefs<"ShelfHeader_shelves">;
}>;

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    plural: true,
    throwOnFieldError: true,
  },
  name: "ShelfHeader_shelves",
  selections: [
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
  ],
  type: "Shelf",
  abstractKey: null,
};

(node as any).hash = "f78743051801202e2a1cfa57293d19aa";

export default node;

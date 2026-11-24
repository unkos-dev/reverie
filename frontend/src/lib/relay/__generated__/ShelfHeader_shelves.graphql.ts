/**
 * @generated SignedSource<<79ec9e539fce50f1803ce4fa1e813c22>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ShelfHeader_shelves$data = ReadonlyArray<{
  readonly id: string;
  readonly itemCount: number;
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
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "itemCount",
      storageKey: null,
    },
  ],
  type: "Shelf",
  abstractKey: null,
};

(node as any).hash = "491c7522dc2f27ea3cd463b35bd57019";

export default node;

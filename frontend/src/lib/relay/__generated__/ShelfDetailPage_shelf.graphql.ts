/**
 * @generated SignedSource<<fedebdf7b1506b529e0a520bdf721d84>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ShelfDetailPage_shelf$data = {
  readonly itemCount: number;
  readonly name: string;
  readonly " $fragmentSpreads": FragmentRefs<"ShelfDetailPage_items">;
  readonly " $fragmentType": "ShelfDetailPage_shelf";
};
export type ShelfDetailPage_shelf$key = {
  readonly " $data"?: ShelfDetailPage_shelf$data;
  readonly " $fragmentSpreads": FragmentRefs<"ShelfDetailPage_shelf">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "ShelfDetailPage_shelf",
  selections: [
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
    {
      args: null,
      kind: "FragmentSpread",
      name: "ShelfDetailPage_items",
    },
  ],
  type: "Shelf",
  abstractKey: null,
};

(node as any).hash = "030220d70dfd2549eff3ba7c7484839a";

export default node;

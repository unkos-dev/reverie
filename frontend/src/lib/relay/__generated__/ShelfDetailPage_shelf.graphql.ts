/**
 * @generated SignedSource<<627571eb2d15b1eb971fe938c0e1ec2a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ShelfDetailPage_shelf$data = {
  readonly isSystem: boolean;
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
      name: "isSystem",
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

(node as any).hash = "4757cd1a12186338d9225099ee9b38d9";

export default node;

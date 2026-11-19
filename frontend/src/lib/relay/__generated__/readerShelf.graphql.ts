/**
 * @generated SignedSource<<066e67bd48899290674bc184df556e05>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelf$data = {
  readonly createdAt: string;
  readonly id: string;
  readonly isSystem: boolean;
  readonly itemCount: number;
  readonly name: string;
  readonly updatedAt: string;
  readonly " $fragmentSpreads": FragmentRefs<"readerShelfItems">;
  readonly " $fragmentType": "readerShelf";
};
export type readerShelf$key = {
  readonly " $data"?: readerShelf$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerShelf">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "readerShelf",
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
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "createdAt",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "updatedAt",
      storageKey: null,
    },
    {
      args: null,
      kind: "FragmentSpread",
      name: "readerShelfItems",
    },
  ],
  type: "Shelf",
  abstractKey: null,
};

(node as any).hash = "85b880417d45708db2a42242c6a41935";

export default node;

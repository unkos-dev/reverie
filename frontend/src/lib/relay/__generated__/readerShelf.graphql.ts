/**
 * @generated SignedSource<<419abe853c235975c25a8dcdeb073e1c>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelf$data = {
  readonly id: string;
  readonly name: string;
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
  metadata: null,
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
      args: null,
      kind: "FragmentSpread",
      name: "readerShelfItems",
    },
  ],
  type: "Shelf",
  abstractKey: null,
};

(node as any).hash = "68ebfe705a75d8bc25a42a34e9e8484e";

export default node;

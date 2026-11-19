/**
 * @generated SignedSource<<0582bac457a570b1d0eb72d48a776392>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSeries$data = {
  readonly id: string;
  readonly name: string;
  readonly " $fragmentSpreads": FragmentRefs<"readerSeriesWorks">;
  readonly " $fragmentType": "readerSeries";
};
export type readerSeries$key = {
  readonly " $data"?: readerSeries$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerSeries">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "readerSeries",
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
      name: "readerSeriesWorks",
    },
  ],
  type: "Series",
  abstractKey: null,
};

(node as any).hash = "f1fde15276e2d6666e9196704200ad36";

export default node;

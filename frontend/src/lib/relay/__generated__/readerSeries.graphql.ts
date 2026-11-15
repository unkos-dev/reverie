/**
 * @generated SignedSource<<00236d177736f4a350cd21c2cd66667c>>
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
  metadata: null,
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

(node as any).hash = "e3cd02e73baa3cbce0cc8793ddc3ee05";

export default node;

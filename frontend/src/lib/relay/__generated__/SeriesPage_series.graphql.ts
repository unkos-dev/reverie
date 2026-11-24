/**
 * @generated SignedSource<<9d40a37a75440c861de3af7c807744af>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type SeriesPage_series$data = {
  readonly name: string;
  readonly " $fragmentSpreads": FragmentRefs<"SeriesPage_works">;
  readonly " $fragmentType": "SeriesPage_series";
};
export type SeriesPage_series$key = {
  readonly " $data"?: SeriesPage_series$data;
  readonly " $fragmentSpreads": FragmentRefs<"SeriesPage_series">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "SeriesPage_series",
  selections: [
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
      name: "SeriesPage_works",
    },
  ],
  type: "Series",
  abstractKey: null,
};

(node as any).hash = "14482cd4a824caf7873645443056ccf8";

export default node;

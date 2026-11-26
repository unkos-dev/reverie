/**
 * @generated SignedSource<<07c09c91ce5378ec66d3ca50963143b5>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type FilterRail_shelfCounts$data = ReadonlyArray<{
  readonly count: number;
  readonly shelfId: string;
  readonly " $fragmentType": "FilterRail_shelfCounts";
}>;
export type FilterRail_shelfCounts$key = ReadonlyArray<{
  readonly " $data"?: FilterRail_shelfCounts$data;
  readonly " $fragmentSpreads": FragmentRefs<"FilterRail_shelfCounts">;
}>;

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    plural: true,
    throwOnFieldError: true,
  },
  name: "FilterRail_shelfCounts",
  selections: [
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "shelfId",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "count",
      storageKey: null,
    },
  ],
  type: "ShelfFacetCount",
  abstractKey: null,
};

(node as any).hash = "0371d7e11ad2681c502cd8adbc8b0c89";

export default node;

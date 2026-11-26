/**
 * @generated SignedSource<<d4d6f86679fac8d4e610b56e6c1c0afd>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type LibraryToolbar_facets$data = {
  readonly matchingCount: number;
  readonly " $fragmentType": "LibraryToolbar_facets";
};
export type LibraryToolbar_facets$key = {
  readonly " $data"?: LibraryToolbar_facets$data;
  readonly " $fragmentSpreads": FragmentRefs<"LibraryToolbar_facets">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "LibraryToolbar_facets",
  selections: [
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "matchingCount",
      storageKey: null,
    },
  ],
  type: "LibraryFacets",
  abstractKey: null,
};

(node as any).hash = "60f5d1dacb33342f85455a1725e56e84";

export default node;

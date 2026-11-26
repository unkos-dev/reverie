/**
 * @generated SignedSource<<cdd2b5b562b35cbc32de532f943c2227>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type LibraryToolbar_summary$data = {
  readonly totalCount: number;
  readonly " $fragmentType": "LibraryToolbar_summary";
};
export type LibraryToolbar_summary$key = {
  readonly " $data"?: LibraryToolbar_summary$data;
  readonly " $fragmentSpreads": FragmentRefs<"LibraryToolbar_summary">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "LibraryToolbar_summary",
  selections: [
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "totalCount",
      storageKey: null,
    },
  ],
  type: "LibrarySummary",
  abstractKey: null,
};

(node as any).hash = "56ecf434e31948593b6a03711ea04abe";

export default node;

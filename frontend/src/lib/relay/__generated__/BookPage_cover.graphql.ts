/**
 * @generated SignedSource<<63a3488a763e41ddfd459ddab8e04363>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type BookPage_cover$data = {
  readonly coverUrl: string;
  readonly id: string;
  readonly " $fragmentType": "BookPage_cover";
};
export type BookPage_cover$key = {
  readonly " $data"?: BookPage_cover$data;
  readonly " $fragmentSpreads": FragmentRefs<"BookPage_cover">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "BookPage_cover",
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
      name: "coverUrl",
      storageKey: null,
    },
  ],
  type: "Manifestation",
  abstractKey: null,
};

(node as any).hash = "9064e37a767aae25a2405d6ef9d3c24e";

export default node;

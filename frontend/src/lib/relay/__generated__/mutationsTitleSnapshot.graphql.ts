/**
 * @generated SignedSource<<2ef16cf9180a061941274ef7880f3615>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type mutationsTitleSnapshot$data = {
  readonly etag: string;
  readonly id: string;
  readonly title: string;
  readonly " $fragmentType": "mutationsTitleSnapshot";
};
export type mutationsTitleSnapshot$key = {
  readonly " $data"?: mutationsTitleSnapshot$data;
  readonly " $fragmentSpreads": FragmentRefs<"mutationsTitleSnapshot">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: null,
  name: "mutationsTitleSnapshot",
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
      name: "etag",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
  ],
  type: "MetadataRevision",
  abstractKey: null,
};

(node as any).hash = "8c3b5e8ff24a674e33881242bf8b58ff";

export default node;

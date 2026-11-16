/**
 * @generated SignedSource<<e7c037935b216756882af9ad903bb07a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type mutationsTitleSnapshot$data = {
  readonly etag: string;
  readonly manifestationId: string;
  readonly title: string;
  readonly workId: string;
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
      name: "workId",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "manifestationId",
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
  type: "MetadataSnapshot",
  abstractKey: null,
};

(node as any).hash = "6ac69a738800ac30c7cad6ec49f9ed0d";

export default node;

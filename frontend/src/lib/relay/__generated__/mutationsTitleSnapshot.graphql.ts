/**
 * @generated SignedSource<<a257e4ef19568403874a1c228b270d8c>>
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
      name: "id",
      storageKey: null,
    },
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
  type: "MetadataRevision",
  abstractKey: null,
};

(node as any).hash = "966ac659cf8d4b1ebd43a28e60c57e87";

export default node;

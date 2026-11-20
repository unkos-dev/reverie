/**
 * @generated SignedSource<<bf7cd5498c69e73b452a4bc74db10841>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type mutationsCellsSnapshot$data = {
  readonly etag: string;
  readonly id: string;
  readonly isbn13: string | null | undefined;
  readonly manifestationId: string;
  readonly pages: number | null | undefined;
  readonly subtitle: string | null | undefined;
  readonly title: string;
  readonly workId: string;
  readonly " $fragmentType": "mutationsCellsSnapshot";
};
export type mutationsCellsSnapshot$key = {
  readonly " $data"?: mutationsCellsSnapshot$data;
  readonly " $fragmentSpreads": FragmentRefs<"mutationsCellsSnapshot">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: null,
  name: "mutationsCellsSnapshot",
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
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
  ],
  type: "MetadataRevision",
  abstractKey: null,
};

(node as any).hash = "51b87896c02e7218c634a1a48ed46c06";

export default node;

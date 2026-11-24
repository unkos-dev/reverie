/**
 * @generated SignedSource<<e93a4a0f93a7ab5cc952251b548784fc>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type EditMetadataDialog_opening$data = {
  readonly description: string | null | undefined;
  readonly etag: string;
  readonly isbn10: string | null | undefined;
  readonly isbn13: string | null | undefined;
  readonly isbnMismatch: boolean;
  readonly language: string | null | undefined;
  readonly publicationDate: string | null | undefined;
  readonly publisher: string | null | undefined;
  readonly subtitle: string | null | undefined;
  readonly title: string;
  readonly " $fragmentType": "EditMetadataDialog_opening";
};
export type EditMetadataDialog_opening$key = {
  readonly " $data"?: EditMetadataDialog_opening$data;
  readonly " $fragmentSpreads": FragmentRefs<"EditMetadataDialog_opening">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "EditMetadataDialog_opening",
  selections: [
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
      name: "description",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "language",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
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
      name: "isbnMismatch",
      storageKey: null,
    },
  ],
  type: "MetadataRevision",
  abstractKey: null,
};

(node as any).hash = "484f9256d85b55f78380538910e08920";

export default node;

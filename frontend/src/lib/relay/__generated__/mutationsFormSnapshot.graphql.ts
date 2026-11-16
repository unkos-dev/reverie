/**
 * @generated SignedSource<<cf4af2176bcf7a394ca0fe86cbfc8b4a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type mutationsFormSnapshot$data = {
  readonly contentRating: string | null | undefined;
  readonly description: string | null | undefined;
  readonly etag: string;
  readonly isbn10: string | null | undefined;
  readonly isbn13: string | null | undefined;
  readonly isbnMismatch: boolean;
  readonly language: string | null | undefined;
  readonly manifestationId: string;
  readonly pages: number | null | undefined;
  readonly publicationDate: string | null | undefined;
  readonly publisher: string | null | undefined;
  readonly subtitle: string | null | undefined;
  readonly title: string;
  readonly workId: string;
  readonly " $fragmentType": "mutationsFormSnapshot";
};
export type mutationsFormSnapshot$key = {
  readonly " $data"?: mutationsFormSnapshot$data;
  readonly " $fragmentSpreads": FragmentRefs<"mutationsFormSnapshot">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: null,
  name: "mutationsFormSnapshot",
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
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "contentRating",
      storageKey: null,
    },
  ],
  type: "MetadataSnapshot",
  abstractKey: null,
};

(node as any).hash = "f4c9b1ee732273562bf07e3327a21aec";

export default node;

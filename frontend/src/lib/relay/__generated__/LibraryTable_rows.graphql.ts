/**
 * @generated SignedSource<<b5c52cd5818b7f116936af81e94d8f73>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type LibraryTable_rows$data = ReadonlyArray<{
  readonly id: string;
  readonly metadata: {
    readonly isbn13: string | null | undefined;
    readonly title: string;
  };
  readonly readingState: {
    readonly rating: number | null | undefined;
  };
  readonly " $fragmentSpreads": FragmentRefs<"LibraryTable_cell">;
  readonly " $fragmentType": "LibraryTable_rows";
}>;
export type LibraryTable_rows$key = ReadonlyArray<{
  readonly " $data"?: LibraryTable_rows$data;
  readonly " $fragmentSpreads": FragmentRefs<"LibraryTable_rows">;
}>;

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    plural: true,
    throwOnFieldError: true,
  },
  name: "LibraryTable_rows",
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
      concreteType: "MetadataRevision",
      kind: "LinkedField",
      name: "metadata",
      plural: false,
      selections: [
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
          name: "isbn13",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      concreteType: "ReadingRevision",
      kind: "LinkedField",
      name: "readingState",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "rating",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    {
      args: null,
      kind: "FragmentSpread",
      name: "LibraryTable_cell",
    },
  ],
  type: "Manifestation",
  abstractKey: null,
};

(node as any).hash = "ca9c9a660f1e55be949a239f29bfed8d";

export default node;

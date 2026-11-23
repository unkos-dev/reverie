/**
 * @generated SignedSource<<e708c427a7cc259b316c47e5b832ed50>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
export type ReadingStatus =
  | "ABANDONED"
  | "FINISHED"
  | "ON_HOLD"
  | "READING"
  | "WANT_TO_READ"
  | "%future added value";
import { FragmentRefs } from "relay-runtime";
export type useCellEdit_batchRows$data = ReadonlyArray<{
  readonly id: string;
  readonly metadata: {
    readonly title: string;
  };
  readonly readingState: {
    readonly etag: string;
    readonly status: ReadingStatus | null | undefined;
  };
  readonly " $fragmentType": "useCellEdit_batchRows";
}>;
export type useCellEdit_batchRows$key = ReadonlyArray<{
  readonly " $data"?: useCellEdit_batchRows$data;
  readonly " $fragmentSpreads": FragmentRefs<"useCellEdit_batchRows">;
}>;

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    plural: true,
    throwOnFieldError: true,
  },
  name: "useCellEdit_batchRows",
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
          name: "etag",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "status",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
  ],
  type: "Manifestation",
  abstractKey: null,
};

(node as any).hash = "106ccca32ac7f08694cd21bdc8a3a824";

export default node;

/**
 * @generated SignedSource<<0bf591ba54c21d48b8e6266804498b05>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type useCellEdit_batchRows$data = ReadonlyArray<{
  readonly id: string;
  readonly readingState: {
    readonly etag: string;
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
      ],
      storageKey: null,
    },
  ],
  type: "Manifestation",
  abstractKey: null,
};

(node as any).hash = "213251b2e48595f2d0398a53a739ca7f";

export default node;

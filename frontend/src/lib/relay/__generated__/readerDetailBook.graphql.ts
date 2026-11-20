/**
 * @generated SignedSource<<1c97138a3e8a64c48e11b943b262bcbb>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerDetailBook$data = {
  readonly coverUrl: string;
  readonly createdAt: string;
  readonly enrichmentStatus: string;
  readonly id: string;
  readonly ingestionStatus: string;
  readonly validationStatus: string;
  readonly versions: {
    readonly pendingCount: number;
  };
  readonly work:
    | {
        readonly id: string;
        readonly selectedSeries:
          | {
              readonly node: {
                readonly id: string;
                readonly name: string;
              };
              readonly position: number | null | undefined;
            }
          | null
          | undefined;
        readonly " $fragmentSpreads": FragmentRefs<"readerWorkAuthors">;
      }
    | null
    | undefined;
  readonly " $fragmentSpreads": FragmentRefs<"readerDetailEditable" | "readerDetailTags">;
  readonly " $fragmentType": "readerDetailBook";
};
export type readerDetailBook$key = {
  readonly " $data"?: readerDetailBook$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerDetailBook">;
};

const node: ReaderFragment = (function () {
  var v0 = {
    alias: null,
    args: null,
    kind: "ScalarField",
    name: "id",
    storageKey: null,
  };
  return {
    argumentDefinitions: [],
    kind: "Fragment",
    metadata: {
      throwOnFieldError: true,
    },
    name: "readerDetailBook",
    selections: [
      v0 /*:: as any*/,
      {
        args: null,
        kind: "FragmentSpread",
        name: "readerDetailEditable",
      },
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "coverUrl",
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "ingestionStatus",
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "validationStatus",
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "enrichmentStatus",
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "createdAt",
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "Work",
        kind: "LinkedField",
        name: "work",
        plural: false,
        selections: [
          v0 /*:: as any*/,
          {
            args: null,
            kind: "FragmentSpread",
            name: "readerWorkAuthors",
          },
          {
            alias: null,
            args: null,
            concreteType: "SelectedSeries",
            kind: "LinkedField",
            name: "selectedSeries",
            plural: false,
            selections: [
              {
                alias: null,
                args: null,
                concreteType: "Series",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: [
                  v0 /*:: as any*/,
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "name",
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "position",
                storageKey: null,
              },
            ],
            storageKey: null,
          },
        ],
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "MetadataVersions",
        kind: "LinkedField",
        name: "versions",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "pendingCount",
            storageKey: null,
          },
        ],
        storageKey: null,
      },
      {
        args: null,
        kind: "FragmentSpread",
        name: "readerDetailTags",
      },
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "7db2591d8e4833d85478fcb2d3126a6a";

export default node;

/**
 * @generated SignedSource<<2bde9de604d75df052f5e5498f120857>>
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
  readonly isbn10: string | null | undefined;
  readonly isbn13: string | null | undefined;
  readonly isbnMismatch: boolean;
  readonly publicationDate: string | null | undefined;
  readonly publisher: string | null | undefined;
  readonly validationStatus: string;
  readonly versions: {
    readonly pendingCount: number;
  };
  readonly work:
    | {
        readonly description: string | null | undefined;
        readonly id: string;
        readonly language: string | null | undefined;
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
        readonly title: string;
        readonly " $fragmentSpreads": FragmentRefs<"readerWorkAuthors">;
      }
    | null
    | undefined;
  readonly " $fragmentSpreads": FragmentRefs<"readerDetailTags">;
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
        name: "isbn13",
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
        name: "isbnMismatch",
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

(node as any).hash = "fb92b34bd41d8fe9c969544bb7413828";

export default node;

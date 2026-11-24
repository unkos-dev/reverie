/**
 * @generated SignedSource<<4f6ec9a9a406366ec0b3641711bb178f>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type BookPage_book$data = {
  readonly id: string;
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
        readonly title: string;
        readonly " $fragmentSpreads": FragmentRefs<"BookPage_authors">;
      }
    | null
    | undefined;
  readonly " $fragmentSpreads": FragmentRefs<
    "BookPage_cover" | "BookPage_overview" | "VersionsTab_book"
  >;
  readonly " $fragmentType": "BookPage_book";
};
export type BookPage_book$key = {
  readonly " $data"?: BookPage_book$data;
  readonly " $fragmentSpreads": FragmentRefs<"BookPage_book">;
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
    name: "BookPage_book",
    selections: [
      v0 /*:: as any*/,
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
            args: null,
            kind: "FragmentSpread",
            name: "BookPage_authors",
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
        args: null,
        kind: "FragmentSpread",
        name: "BookPage_overview",
      },
      {
        args: null,
        kind: "FragmentSpread",
        name: "BookPage_cover",
      },
      {
        args: null,
        kind: "FragmentSpread",
        name: "VersionsTab_book",
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
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "010c5ec88dc34599021e2bfa4ca7355e";

export default node;

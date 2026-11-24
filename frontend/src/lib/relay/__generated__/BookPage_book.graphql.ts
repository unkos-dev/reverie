/**
 * @generated SignedSource<<e537bee72b2ffc27c729b38f8948caf8>>
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
    readonly __id: string;
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
          {
            kind: "ClientExtension",
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "__id",
                storageKey: null,
              },
            ],
          },
        ],
        storageKey: null,
      },
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "569e6e6fe7eb9eff84babc754edda00e";

export default node;

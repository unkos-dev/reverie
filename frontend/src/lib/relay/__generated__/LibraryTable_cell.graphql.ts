/**
 * @generated SignedSource<<cdc4dc7afe2c0c4c5a6ffc39a75896b6>>
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
export type LibraryTable_cell$data = {
  readonly coverUrl: string;
  readonly createdAt: string;
  readonly id: string;
  readonly metadata: {
    readonly etag: string;
    readonly isbn13: string | null | undefined;
    readonly pages: number | null | undefined;
    readonly subtitle: string | null | undefined;
    readonly title: string;
  };
  readonly readingState: {
    readonly etag: string;
    readonly rating: number | null | undefined;
    readonly status: ReadingStatus | null | undefined;
  };
  readonly work:
    | {
        readonly authors: {
          readonly edges: ReadonlyArray<{
            readonly node: {
              readonly name: string;
            };
          }>;
        };
        readonly id: string;
        readonly selectedSeries:
          | {
              readonly node: {
                readonly name: string;
              };
              readonly position: number | null | undefined;
            }
          | null
          | undefined;
        readonly subtitle: string | null | undefined;
        readonly title: string;
      }
    | null
    | undefined;
  readonly " $fragmentType": "LibraryTable_cell";
};
export type LibraryTable_cell$key = {
  readonly " $data"?: LibraryTable_cell$data;
  readonly " $fragmentSpreads": FragmentRefs<"LibraryTable_cell">;
};

const node: ReaderFragment = (function () {
  var v0 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v3 = [
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "name",
        storageKey: null,
      },
    ],
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    };
  return {
    argumentDefinitions: [],
    kind: "Fragment",
    metadata: {
      throwOnFieldError: true,
    },
    name: "LibraryTable_cell",
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
          v1 /*:: as any*/,
          v2 /*:: as any*/,
          {
            alias: null,
            args: [
              {
                kind: "Literal",
                name: "first",
                value: 5,
              },
            ],
            concreteType: "AuthorConnection",
            kind: "LinkedField",
            name: "authors",
            plural: false,
            selections: [
              {
                alias: null,
                args: null,
                concreteType: "AuthorEdge",
                kind: "LinkedField",
                name: "edges",
                plural: true,
                selections: [
                  {
                    alias: null,
                    args: null,
                    concreteType: "Author",
                    kind: "LinkedField",
                    name: "node",
                    plural: false,
                    selections: v3 /*:: as any*/,
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
            ],
            storageKey: "authors(first:5)",
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
                selections: v3 /*:: as any*/,
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
        concreteType: "MetadataRevision",
        kind: "LinkedField",
        name: "metadata",
        plural: false,
        selections: [
          v4 /*:: as any*/,
          v1 /*:: as any*/,
          v2 /*:: as any*/,
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
          v4 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "status",
            storageKey: null,
          },
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
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "f693017b4b57c67ec388bdfb74e8faf5";

export default node;

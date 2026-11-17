/**
 * @generated SignedSource<<a34c7c2ef596257afece32741020130a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerLibraryBook$data = ReadonlyArray<{
  readonly coverUrl: string;
  readonly createdAt: string;
  readonly id: string;
  readonly isbn13: string | null | undefined;
  readonly metadata: {
    readonly etag: string;
    readonly isbn13: string | null | undefined;
    readonly manifestationId: string;
    readonly pages: number | null | undefined;
    readonly subtitle: string | null | undefined;
    readonly title: string;
    readonly workId: string;
  };
  readonly pages: number | null | undefined;
  readonly readingState: {
    readonly etag: string;
    readonly finishedAt: string | null | undefined;
    readonly progressPct: number | null | undefined;
    readonly rating: number | null | undefined;
    readonly startedAt: string | null | undefined;
    readonly status: string | null | undefined;
  };
  readonly work:
    | {
        readonly authors: {
          readonly edges: ReadonlyArray<{
            readonly node: {
              readonly name: string;
            };
          }>;
          readonly pageInfo: {
            readonly endCursor: string | null | undefined;
            readonly hasNextPage: boolean;
          };
        };
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
        readonly subtitle: string | null | undefined;
        readonly title: string;
      }
    | null
    | undefined;
  readonly " $fragmentType": "readerLibraryBook";
}>;
export type readerLibraryBook$key = ReadonlyArray<{
  readonly " $data"?: readerLibraryBook$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerLibraryBook">;
}>;

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
      name: "isbn13",
      storageKey: null,
    },
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v6 = {
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
      plural: true,
    },
    name: "readerLibraryBook",
    selections: [
      v0 /*:: as any*/,
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "coverUrl",
        storageKey: null,
      },
      v1 /*:: as any*/,
      v2 /*:: as any*/,
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
          v3 /*:: as any*/,
          v4 /*:: as any*/,
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
                    selections: [v5 /*:: as any*/],
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                concreteType: "PageInfo",
                kind: "LinkedField",
                name: "pageInfo",
                plural: false,
                selections: [
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "hasNextPage",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "endCursor",
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
                selections: [v0 /*:: as any*/, v5 /*:: as any*/],
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
        concreteType: "MetadataSnapshot",
        kind: "LinkedField",
        name: "metadata",
        plural: false,
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
          v6 /*:: as any*/,
          v3 /*:: as any*/,
          v4 /*:: as any*/,
          v1 /*:: as any*/,
          v2 /*:: as any*/,
        ],
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "ReadingState",
        kind: "LinkedField",
        name: "readingState",
        plural: false,
        selections: [
          v6 /*:: as any*/,
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
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "progressPct",
            storageKey: null,
          },
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "startedAt",
            storageKey: null,
          },
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "finishedAt",
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

(node as any).hash = "c2dadf3aa9e423342c31172a8bb77298";

export default node;

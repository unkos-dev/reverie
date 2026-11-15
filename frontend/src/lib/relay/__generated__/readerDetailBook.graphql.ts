/**
 * @generated SignedSource<<8dfd73827a020a413ec11c5c336239db>>
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
  readonly tags: {
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
  readonly validationStatus: string;
  readonly versions: {
    readonly pendingCount: number;
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
      }
    | null
    | undefined;
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
    },
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v2 = [v1 /*:: as any*/],
    v3 = {
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
    };
  return {
    argumentDefinitions: [],
    kind: "Fragment",
    metadata: null,
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
                    selections: v2 /*:: as any*/,
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
              v3 /*:: as any*/,
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
                selections: [v0 /*:: as any*/, v1 /*:: as any*/],
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
        args: [
          {
            kind: "Literal",
            name: "first",
            value: 10,
          },
        ],
        concreteType: "TagConnection",
        kind: "LinkedField",
        name: "tags",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "TagEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              {
                alias: null,
                args: null,
                concreteType: "Tag",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: v2 /*:: as any*/,
                storageKey: null,
              },
            ],
            storageKey: null,
          },
          v3 /*:: as any*/,
        ],
        storageKey: "tags(first:10)",
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

(node as any).hash = "dffbc75682a11d05329a84e556f399e7";

export default node;

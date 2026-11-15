/**
 * @generated SignedSource<<7c4317d11696799068dc2900c6f7302f>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerBook$data = {
  readonly contentRating: string | null | undefined;
  readonly coverUrl: string;
  readonly createdAt: string;
  readonly enrichmentStatus: string;
  readonly genres: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly id: string;
        readonly name: string;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
  readonly id: string;
  readonly identifiers: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly level: string;
        readonly scheme: string;
        readonly value: string;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
  readonly ingestionStatus: string;
  readonly isbn10: string | null | undefined;
  readonly isbn13: string | null | undefined;
  readonly isbnMismatch: boolean;
  readonly metadata: {
    readonly contentRating: string | null | undefined;
    readonly description: string | null | undefined;
    readonly etag: string;
    readonly isbn10: string | null | undefined;
    readonly isbn13: string | null | undefined;
    readonly language: string | null | undefined;
    readonly manifestationId: string;
    readonly pages: number | null | undefined;
    readonly publicationDate: string | null | undefined;
    readonly publisher: string | null | undefined;
    readonly subtitle: string | null | undefined;
    readonly title: string;
    readonly workId: string;
  };
  readonly moods: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly id: string;
        readonly name: string;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
  readonly pages: number | null | undefined;
  readonly publicationDate: string | null | undefined;
  readonly publisher: string | null | undefined;
  readonly ratings: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly fetchedAt: string | null | undefined;
        readonly rating: number;
        readonly reviewCount: number | null | undefined;
        readonly scale: number;
        readonly source: string;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
  readonly readingState: {
    readonly etag: string;
    readonly finishedAt: string | null | undefined;
    readonly lastReadAt: string | null | undefined;
    readonly notes: string | null | undefined;
    readonly progressPct: number | null | undefined;
    readonly rating: number | null | undefined;
    readonly startedAt: string | null | undefined;
    readonly status: string | null | undefined;
  };
  readonly tags: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly id: string;
        readonly name: string;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
  readonly updatedAt: string;
  readonly validationStatus: string;
  readonly work:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerWork">;
      }
    | null
    | undefined;
  readonly " $fragmentType": "readerBook";
};
export type readerBook$key = {
  readonly " $data"?: readerBook$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerBook">;
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
      name: "isbn13",
      storageKey: null,
    },
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "contentRating",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "rating",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v10 = [
      v0 /*:: as any*/,
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "name",
        storageKey: null,
      },
    ],
    v11 = {
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
          name: "hasPreviousPage",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "startCursor",
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
    name: "readerBook",
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
          {
            args: null,
            kind: "FragmentSpread",
            name: "readerWork",
          },
        ],
        storageKey: null,
      },
      v1 /*:: as any*/,
      v2 /*:: as any*/,
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "isbnMismatch",
        storageKey: null,
      },
      v3 /*:: as any*/,
      v4 /*:: as any*/,
      v5 /*:: as any*/,
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
        kind: "ScalarField",
        name: "updatedAt",
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "coverUrl",
        storageKey: null,
      },
      v6 /*:: as any*/,
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
          v7 /*:: as any*/,
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
            name: "subtitle",
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
          v2 /*:: as any*/,
          v1 /*:: as any*/,
          v3 /*:: as any*/,
          v4 /*:: as any*/,
          v5 /*:: as any*/,
          v6 /*:: as any*/,
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
          v7 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "status",
            storageKey: null,
          },
          v8 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "notes",
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
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "lastReadAt",
            storageKey: null,
          },
        ],
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "GenreConnection",
        kind: "LinkedField",
        name: "genres",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "GenreEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              v9 /*:: as any*/,
              {
                alias: null,
                args: null,
                concreteType: "Genre",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: v10 /*:: as any*/,
                storageKey: null,
              },
            ],
            storageKey: null,
          },
          v11 /*:: as any*/,
        ],
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "MoodConnection",
        kind: "LinkedField",
        name: "moods",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "MoodEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              v9 /*:: as any*/,
              {
                alias: null,
                args: null,
                concreteType: "Mood",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: v10 /*:: as any*/,
                storageKey: null,
              },
            ],
            storageKey: null,
          },
          v11 /*:: as any*/,
        ],
        storageKey: null,
      },
      {
        alias: null,
        args: null,
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
              v9 /*:: as any*/,
              {
                alias: null,
                args: null,
                concreteType: "Tag",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: v10 /*:: as any*/,
                storageKey: null,
              },
            ],
            storageKey: null,
          },
          v11 /*:: as any*/,
        ],
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "ExternalIdentifierConnection",
        kind: "LinkedField",
        name: "identifiers",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "ExternalIdentifierEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              v9 /*:: as any*/,
              {
                alias: null,
                args: null,
                concreteType: "ExternalIdentifier",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: [
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "level",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "scheme",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "value",
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
            ],
            storageKey: null,
          },
          v11 /*:: as any*/,
        ],
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "ExternalRatingConnection",
        kind: "LinkedField",
        name: "ratings",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "ExternalRatingEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              v9 /*:: as any*/,
              {
                alias: null,
                args: null,
                concreteType: "ExternalRating",
                kind: "LinkedField",
                name: "node",
                plural: false,
                selections: [
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "source",
                    storageKey: null,
                  },
                  v8 /*:: as any*/,
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "scale",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "reviewCount",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "fetchedAt",
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
            ],
            storageKey: null,
          },
          v11 /*:: as any*/,
        ],
        storageKey: null,
      },
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "eaaf2f4175f6e9f6dabe9ff7319bcc1b";

export default node;

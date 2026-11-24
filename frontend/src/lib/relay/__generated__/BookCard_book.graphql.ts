/**
 * @generated SignedSource<<6217d7656f5378eed722025726ab176f>>
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
export type BookCard_book$data = {
  readonly coverUrl: string;
  readonly id: string;
  readonly metadata: {
    readonly title: string;
  };
  readonly readingState: {
    readonly progressPct: number | null | undefined;
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
        readonly title: string;
      }
    | null
    | undefined;
  readonly " $fragmentType": "BookCard_book";
};
export type BookCard_book$key = {
  readonly " $data"?: BookCard_book$data;
  readonly " $fragmentSpreads": FragmentRefs<"BookCard_book">;
};

const node: ReaderFragment = (function () {
  var v0 = {
    alias: null,
    args: null,
    kind: "ScalarField",
    name: "title",
    storageKey: null,
  };
  return {
    argumentDefinitions: [],
    kind: "Fragment",
    metadata: {
      throwOnFieldError: true,
    },
    name: "BookCard_book",
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
        kind: "ScalarField",
        name: "coverUrl",
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "MetadataRevision",
        kind: "LinkedField",
        name: "metadata",
        plural: false,
        selections: [v0 /*:: as any*/],
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
                    selections: [
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
                ],
                storageKey: null,
              },
            ],
            storageKey: "authors(first:5)",
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
            name: "status",
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

(node as any).hash = "90fb63cdd0e3c563e5e00802b0f9baab";

export default node;

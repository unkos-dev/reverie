/**
 * @generated SignedSource<<0fdc993bb9c98a57e3d0a39075fc50d5>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerDetailEditable$data = {
  readonly id: string;
  readonly isbn10: string | null | undefined;
  readonly isbn13: string | null | undefined;
  readonly isbnMismatch: boolean;
  readonly publicationDate: string | null | undefined;
  readonly publisher: string | null | undefined;
  readonly work:
    | {
        readonly description: string | null | undefined;
        readonly id: string;
        readonly language: string | null | undefined;
        readonly title: string;
      }
    | null
    | undefined;
  readonly " $fragmentType": "readerDetailEditable";
};
export type readerDetailEditable$key = {
  readonly " $data"?: readerDetailEditable$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerDetailEditable">;
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
    name: "readerDetailEditable",
    selections: [
      v0 /*:: as any*/,
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
        ],
        storageKey: null,
      },
    ],
    type: "Manifestation",
    abstractKey: null,
  };
})();

(node as any).hash = "566b3ed923190fc8b55c8f5a7c8b6332";

export default node;

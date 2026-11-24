/**
 * @generated SignedSource<<6756da979b8c9552bb6cf5194b071ccf>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type VersionsTab_book$data = {
  readonly id: string;
  readonly isbn10: string | null | undefined;
  readonly isbn13: string | null | undefined;
  readonly publicationDate: string | null | undefined;
  readonly publisher: string | null | undefined;
  readonly work:
    | {
        readonly description: string | null | undefined;
        readonly language: string | null | undefined;
        readonly title: string;
      }
    | null
    | undefined;
  readonly " $fragmentType": "VersionsTab_book";
};
export type VersionsTab_book$key = {
  readonly " $data"?: VersionsTab_book$data;
  readonly " $fragmentSpreads": FragmentRefs<"VersionsTab_book">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "VersionsTab_book",
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
      name: "isbn10",
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

(node as any).hash = "42e1aac4c8f4bd66c5098e4d55163ae3";

export default node;

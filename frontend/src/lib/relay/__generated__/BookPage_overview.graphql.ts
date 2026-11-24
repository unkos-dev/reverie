/**
 * @generated SignedSource<<6b9b61f9c9f1cd980c9e460514ccaeea>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
export type EnrichmentStatus =
  | "COMPLETE"
  | "FAILED"
  | "IN_PROGRESS"
  | "PENDING"
  | "SKIPPED"
  | "%future added value";
export type IngestionStatus =
  | "COMPLETE"
  | "FAILED"
  | "PENDING"
  | "PROCESSING"
  | "SKIPPED"
  | "%future added value";
export type ValidationStatus =
  | "CLEAN"
  | "DEGRADED"
  | "FAILED"
  | "PENDING"
  | "REPAIRED"
  | "%future added value";
import { FragmentRefs } from "relay-runtime";
export type BookPage_overview$data = {
  readonly enrichmentStatus: EnrichmentStatus;
  readonly ingestionStatus: IngestionStatus;
  readonly isbn10: string | null | undefined;
  readonly isbn13: string | null | undefined;
  readonly isbnMismatch: boolean;
  readonly validationStatus: ValidationStatus;
  readonly work:
    | {
        readonly description: string | null | undefined;
        readonly language: string | null | undefined;
      }
    | null
    | undefined;
  readonly " $fragmentSpreads": FragmentRefs<"BookPage_tags">;
  readonly " $fragmentType": "BookPage_overview";
};
export type BookPage_overview$key = {
  readonly " $data"?: BookPage_overview$data;
  readonly " $fragmentSpreads": FragmentRefs<"BookPage_overview">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "BookPage_overview",
  selections: [
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
      concreteType: "Work",
      kind: "LinkedField",
      name: "work",
      plural: false,
      selections: [
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
    {
      args: null,
      kind: "FragmentSpread",
      name: "BookPage_tags",
    },
  ],
  type: "Manifestation",
  abstractKey: null,
};

(node as any).hash = "d9a15c4002879a42ece2f078b7d82f93";

export default node;

/**
 * @generated SignedSource<<07f66b0d857ff6f6d9ddd50164e35720>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ShelfHeader_summary$data = {
  readonly needsAttention: {
    readonly enrichmentFailed: number;
    readonly noCover: number;
    readonly noIsbn: number;
    readonly toReview: number;
  };
  readonly readingStates: {
    readonly finished: number;
    readonly reading: number;
    readonly unread: number;
  };
  readonly totalCount: number;
  readonly " $fragmentType": "ShelfHeader_summary";
};
export type ShelfHeader_summary$key = {
  readonly " $data"?: ShelfHeader_summary$data;
  readonly " $fragmentSpreads": FragmentRefs<"ShelfHeader_summary">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "ShelfHeader_summary",
  selections: [
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "totalCount",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      concreteType: "ReadingStateCounts",
      kind: "LinkedField",
      name: "readingStates",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "finished",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "reading",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "unread",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      concreteType: "NeedsAttentionCounts",
      kind: "LinkedField",
      name: "needsAttention",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "toReview",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "enrichmentFailed",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "noIsbn",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "noCover",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
  ],
  type: "LibrarySummary",
  abstractKey: null,
};

(node as any).hash = "13593bd95fcc29d4ef32e72946c3ce3a";

export default node;

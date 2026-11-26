/**
 * @generated SignedSource<<1104b3add73fed9005965717cad0988f>>
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
    readonly abandoned: number;
    readonly finished: number;
    readonly onHold: number;
    readonly reading: number;
    readonly unread: number;
    readonly wantToRead: number;
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
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "wantToRead",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "onHold",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "abandoned",
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

(node as any).hash = "53730a5a8ff5a577f8146bcb74f75e38";

export default node;

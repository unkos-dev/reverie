/**
 * @generated SignedSource<<69c80f66050628302afe3bce68030cb5>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type editors_statusCounts$data = {
  readonly abandoned: number;
  readonly finished: number;
  readonly onHold: number;
  readonly reading: number;
  readonly unread: number;
  readonly wantToRead: number;
  readonly " $fragmentType": "editors_statusCounts";
};
export type editors_statusCounts$key = {
  readonly " $data"?: editors_statusCounts$data;
  readonly " $fragmentSpreads": FragmentRefs<"editors_statusCounts">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "editors_statusCounts",
  selections: [
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
      name: "reading",
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
      name: "finished",
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
  type: "ReadingStateCounts",
  abstractKey: null,
};

(node as any).hash = "4e2bfdbed04893d3fca464912fd2b748";

export default node;

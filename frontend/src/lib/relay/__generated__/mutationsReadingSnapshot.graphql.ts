/**
 * @generated SignedSource<<64549167830a1902171be6d25fa398d1>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type mutationsReadingSnapshot$data = {
  readonly etag: string;
  readonly finishedAt: string | null | undefined;
  readonly lastReadAt: string | null | undefined;
  readonly notes: string | null | undefined;
  readonly progressPct: number | null | undefined;
  readonly rating: number | null | undefined;
  readonly startedAt: string | null | undefined;
  readonly status: string | null | undefined;
  readonly " $fragmentType": "mutationsReadingSnapshot";
};
export type mutationsReadingSnapshot$key = {
  readonly " $data"?: mutationsReadingSnapshot$data;
  readonly " $fragmentSpreads": FragmentRefs<"mutationsReadingSnapshot">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: null,
  name: "mutationsReadingSnapshot",
  selections: [
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
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
  type: "ReadingState",
  abstractKey: null,
};

(node as any).hash = "a5bcc9f23e1f9b5292050d672e882f91";

export default node;

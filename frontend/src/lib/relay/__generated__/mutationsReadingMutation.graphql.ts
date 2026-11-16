/**
 * @generated SignedSource<<61ada95e7fced585c67dfb937a5c9b76>>
 * @relayHash 08dbff986fa62be95175d70dc203ce34
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID a8322b155e33596862f8966419906a6e34ca46195fdbc94397fdcad6a60fac17

import { ConcreteRequest } from "relay-runtime";
export type ReadingStatusInput =
  | "ABANDONED"
  | "FINISHED"
  | "ON_HOLD"
  | "READING"
  | "WANT_TO_READ"
  | "%future added value";
export type ReadingPatch = {
  notes?: string | null | undefined;
  rating?: number | null | undefined;
  status?: ReadingStatusInput | null | undefined;
};
export type mutationsReadingMutation$variables = {
  baseline?: string | null | undefined;
  id: string;
  patch: ReadingPatch;
};
export type mutationsReadingMutation$data = {
  readonly updateReadingState: {
    readonly etag: string;
    readonly finishedAt: string | null | undefined;
    readonly lastReadAt: string | null | undefined;
    readonly notes: string | null | undefined;
    readonly progressPct: number | null | undefined;
    readonly rating: number | null | undefined;
    readonly startedAt: string | null | undefined;
    readonly status: string | null | undefined;
  };
};
export type mutationsReadingMutation = {
  response: mutationsReadingMutation$data;
  variables: mutationsReadingMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "baseline",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "patch",
    },
    v3 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "baseline",
            variableName: "baseline",
          },
          {
            kind: "Variable",
            name: "id",
            variableName: "id",
          },
          {
            kind: "Variable",
            name: "patch",
            variableName: "patch",
          },
        ],
        concreteType: "ReadingState",
        kind: "LinkedField",
        name: "updateReadingState",
        plural: false,
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
        storageKey: null,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsReadingMutation",
      selections: v3 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v2 /*:: as any*/],
      kind: "Operation",
      name: "mutationsReadingMutation",
      selections: v3 /*:: as any*/,
    },
    params: {
      id: "a8322b155e33596862f8966419906a6e34ca46195fdbc94397fdcad6a60fac17",
      metadata: {},
      name: "mutationsReadingMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "d8ad7624df01e29026f5d286cab6fb39";

export default node;

/**
 * @generated SignedSource<<c0872738351bfa2f50053d78dc8a00d6>>
 * @relayHash a79daae3ec2e5221862187a1f656712f
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 671426667a71f9d1d1c3ccf3a00e3fd7d64e0dff644587f3fba135c4ceb08d53

import { ConcreteRequest } from "relay-runtime";
export type readerAuthorLabelsQuery$variables = {
  ids: ReadonlyArray<string>;
};
export type readerAuthorLabelsQuery$data = {
  readonly authorLabels: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
};
export type readerAuthorLabelsQuery = {
  response: readerAuthorLabelsQuery$data;
  variables: readerAuthorLabelsQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "ids",
      },
    ],
    v1 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "ids",
            variableName: "ids",
          },
        ],
        concreteType: "AuthorSuggestion",
        kind: "LinkedField",
        name: "authorLabels",
        plural: true,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "entityId",
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
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerAuthorLabelsQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerAuthorLabelsQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "671426667a71f9d1d1c3ccf3a00e3fd7d64e0dff644587f3fba135c4ceb08d53",
      metadata: {},
      name: "readerAuthorLabelsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "85521250737e9281f1d5bd205d3bd8a4";

export default node;

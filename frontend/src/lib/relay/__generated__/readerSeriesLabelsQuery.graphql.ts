/**
 * @generated SignedSource<<259fc5f4f8d0ad39ae593c77b919126d>>
 * @relayHash fd41d7325d16ac292f3d70939d7cbccb
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID b9ecaf564e448d652cf6ac34a25f74411cb4c929d3d991d61b815624c18b6d63

import { ConcreteRequest } from "relay-runtime";
export type readerSeriesLabelsQuery$variables = {
  ids: ReadonlyArray<string>;
};
export type readerSeriesLabelsQuery$data = {
  readonly seriesLabels: ReadonlyArray<{
    readonly entityId: string | null | undefined;
    readonly value: string;
  }>;
};
export type readerSeriesLabelsQuery = {
  response: readerSeriesLabelsQuery$data;
  variables: readerSeriesLabelsQuery$variables;
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
        concreteType: "SeriesSuggestion",
        kind: "LinkedField",
        name: "seriesLabels",
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
      name: "readerSeriesLabelsQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerSeriesLabelsQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "b9ecaf564e448d652cf6ac34a25f74411cb4c929d3d991d61b815624c18b6d63",
      metadata: {},
      name: "readerSeriesLabelsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "4c3273efb2ee8edf2911c96731525093";

export default node;

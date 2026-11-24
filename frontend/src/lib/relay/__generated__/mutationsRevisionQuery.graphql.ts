/**
 * @generated SignedSource<<ba8b97d194b5799ccc1138ed403b177b>>
 * @relayHash 0789d3fced02b0d19a93df7783395c53
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 196ce41a24d4aab9f7e6e35d80158b852b7684a921654f86919473d700469f84

import { ConcreteRequest } from "relay-runtime";
export type mutationsRevisionQuery$variables = {
  id: string;
};
export type mutationsRevisionQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly metadata: {
          readonly etag: string;
        };
      }
    | null
    | undefined;
};
export type mutationsRevisionQuery = {
  response: mutationsRevisionQuery$data;
  variables: mutationsRevisionQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "id",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsRevisionQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [v3 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsRevisionQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [v3 /*:: as any*/, v2 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "196ce41a24d4aab9f7e6e35d80158b852b7684a921654f86919473d700469f84",
      metadata: {},
      name: "mutationsRevisionQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "5e89b0ad113fae4380b515d4c5acd5b7";

export default node;

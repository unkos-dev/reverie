/**
 * @generated SignedSource<<ef61a06ee08becbe4e886602adb61283>>
 * @relayHash 05ab1ee9b08133e6c9e94754d6b0ce94
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 9fc49902f0e889f6a48f1637e8bc9644fee3129ebe613c0843a0b8d944041b71

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
          readonly manifestationId: string;
          readonly workId: string;
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
      name: "workId",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "manifestationId",
      storageKey: null,
    },
    v5 = {
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
              selections: [v3 /*:: as any*/, v4 /*:: as any*/, v5 /*:: as any*/],
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
              selections: [v3 /*:: as any*/, v4 /*:: as any*/, v5 /*:: as any*/, v2 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "9fc49902f0e889f6a48f1637e8bc9644fee3129ebe613c0843a0b8d944041b71",
      metadata: {},
      name: "mutationsRevisionQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "6bf0fbf7a589e22ed5524869c6139351";

export default node;

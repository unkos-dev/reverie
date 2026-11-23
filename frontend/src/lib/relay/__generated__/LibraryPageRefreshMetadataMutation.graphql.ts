/**
 * @generated SignedSource<<f8ed842014fb6c3346aae2d126c0e962>>
 * @relayHash 677bd6e3891952bbaa7e6f6d82b3d4a6
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID e9ce5139801645e211b87757b6abf2876ed30dd565e2de36f41d4379bab69bf9

import { ConcreteRequest } from "relay-runtime";
export type RefreshMetadataInput = {
  id: string;
};
export type LibraryPageRefreshMetadataMutation$variables = {
  input: RefreshMetadataInput;
};
export type LibraryPageRefreshMetadataMutation$data = {
  readonly refreshMetadata:
    | {
        readonly __typename: "MetadataRefreshQueued";
        readonly manifestationId: string;
      }
    | {
        readonly __typename: "NotFound";
        readonly message: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type LibraryPageRefreshMetadataMutation = {
  response: LibraryPageRefreshMetadataMutation$data;
  variables: LibraryPageRefreshMetadataMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "input",
      },
    ],
    v1 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "input",
            variableName: "input",
          },
        ],
        concreteType: null,
        kind: "LinkedField",
        name: "refreshMetadata",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "__typename",
            storageKey: null,
          },
          {
            kind: "InlineFragment",
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "manifestationId",
                storageKey: null,
              },
            ],
            type: "MetadataRefreshQueued",
            abstractKey: null,
          },
          {
            kind: "InlineFragment",
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "message",
                storageKey: null,
              },
            ],
            type: "NotFound",
            abstractKey: null,
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
      name: "LibraryPageRefreshMetadataMutation",
      selections: v1 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "LibraryPageRefreshMetadataMutation",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "e9ce5139801645e211b87757b6abf2876ed30dd565e2de36f41d4379bab69bf9",
      metadata: {},
      name: "LibraryPageRefreshMetadataMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "351d8793e29b1fc871c8a5a05a6a3bab";

export default node;

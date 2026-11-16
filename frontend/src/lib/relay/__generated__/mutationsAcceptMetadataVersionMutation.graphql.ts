/**
 * @generated SignedSource<<a9e204efb5c58286cedb02eee1fd47fd>>
 * @relayHash ba74df637488d9a240ade4ad082d393f
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 6677f79cc5be38987d876a1840861b2e9634364b4a780b589e0fb0254ddf795e

import { ConcreteRequest } from "relay-runtime";
export type mutationsAcceptMetadataVersionMutation$variables = {
  id: string;
  versionId: string;
};
export type mutationsAcceptMetadataVersionMutation$data = {
  readonly acceptMetadataVersion: {
    readonly manifestationId: string;
    readonly metadata:
      | {
          readonly contentRating: string | null | undefined;
          readonly description: string | null | undefined;
          readonly etag: string;
          readonly isbn10: string | null | undefined;
          readonly isbn13: string | null | undefined;
          readonly isbnMismatch: boolean;
          readonly language: string | null | undefined;
          readonly manifestationId: string;
          readonly pages: number | null | undefined;
          readonly publicationDate: string | null | undefined;
          readonly publisher: string | null | undefined;
          readonly subtitle: string | null | undefined;
          readonly title: string;
          readonly workId: string;
        }
      | null
      | undefined;
  };
};
export type mutationsAcceptMetadataVersionMutation = {
  response: mutationsAcceptMetadataVersionMutation$data;
  variables: mutationsAcceptMetadataVersionMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "id",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "versionId",
      },
    ],
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "manifestationId",
      storageKey: null,
    },
    v2 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "id",
            variableName: "id",
          },
          {
            kind: "Variable",
            name: "versionId",
            variableName: "versionId",
          },
        ],
        concreteType: "MetadataReview",
        kind: "LinkedField",
        name: "acceptMetadataVersion",
        plural: false,
        selections: [
          v1 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "MetadataSnapshot",
            kind: "LinkedField",
            name: "metadata",
            plural: false,
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "workId",
                storageKey: null,
              },
              v1 /*:: as any*/,
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
                name: "title",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "subtitle",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "description",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "language",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "publisher",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "publicationDate",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "isbn10",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "isbn13",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "isbnMismatch",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "pages",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "contentRating",
                storageKey: null,
              },
            ],
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
      name: "mutationsAcceptMetadataVersionMutation",
      selections: v2 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsAcceptMetadataVersionMutation",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "6677f79cc5be38987d876a1840861b2e9634364b4a780b589e0fb0254ddf795e",
      metadata: {},
      name: "mutationsAcceptMetadataVersionMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "0d47245e89bb9b4d96c85e0b3b03432e";

export default node;

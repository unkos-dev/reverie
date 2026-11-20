/**
 * @generated SignedSource<<614e21b0ac9b62416ba56877a0a6eb3e>>
 * @relayHash 57d409c00d8b857f6aae9fc76886d6a1
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 974612d866b5f1a5d9db0f1cad81a0b97f8d175d20bad9fe36bc664ddbce7a2c

import { ConcreteRequest } from "relay-runtime";
export type mutationsRevertMetadataFieldMutation$variables = {
  fieldName: string;
  id: string;
  versionId?: string | null | undefined;
};
export type mutationsRevertMetadataFieldMutation$data = {
  readonly revertMetadataField: {
    readonly manifestationId: string;
    readonly metadata:
      | {
          readonly contentRating: string | null | undefined;
          readonly description: string | null | undefined;
          readonly etag: string;
          readonly id: string;
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
export type mutationsRevertMetadataFieldMutation = {
  response: mutationsRevertMetadataFieldMutation$data;
  variables: mutationsRevertMetadataFieldMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "fieldName",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "versionId",
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "manifestationId",
      storageKey: null,
    },
    v4 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "fieldName",
            variableName: "fieldName",
          },
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
        name: "revertMetadataField",
        plural: false,
        selections: [
          v3 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "MetadataRevision",
            kind: "LinkedField",
            name: "metadata",
            plural: false,
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "id",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "workId",
                storageKey: null,
              },
              v3 /*:: as any*/,
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
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsRevertMetadataFieldMutation",
      selections: v4 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v2 /*:: as any*/],
      kind: "Operation",
      name: "mutationsRevertMetadataFieldMutation",
      selections: v4 /*:: as any*/,
    },
    params: {
      id: "974612d866b5f1a5d9db0f1cad81a0b97f8d175d20bad9fe36bc664ddbce7a2c",
      metadata: {},
      name: "mutationsRevertMetadataFieldMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "21e0c87ff0a1d91cfc1cf17aa5ecf371";

export default node;

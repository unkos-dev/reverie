/**
 * @generated SignedSource<<28fd67c9bf7617e99a3183dc5ba50184>>
 * @relayHash 1062ce7bc9b4fa1ce689fa675ba16535
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID d61853301e5828102dbfb4871fefeadf0e6cae8477cdd75603051efb5b371563

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
      id: "d61853301e5828102dbfb4871fefeadf0e6cae8477cdd75603051efb5b371563",
      metadata: {},
      name: "mutationsRevertMetadataFieldMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "21e0c87ff0a1d91cfc1cf17aa5ecf371";

export default node;

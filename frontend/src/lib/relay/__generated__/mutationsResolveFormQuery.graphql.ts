/**
 * @generated SignedSource<<0480bd2933eae5db25e988eac61dbc9e>>
 * @relayHash 1f85a67253fb464323c933020f6b448c
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 15e9cc9dca8d51c00953141bbb203ad50cc3c0dbc050843534702a1ad31e5e16

import { ConcreteRequest } from "relay-runtime";
export type mutationsResolveFormQuery$variables = {
  id: string;
  operationId?: string | null | undefined;
  requestId?: string | null | undefined;
};
export type mutationsResolveFormQuery$data = {
  readonly metadataOperation: {
    readonly metadata: {
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
    };
    readonly operations: ReadonlyArray<{
      readonly expiresAt: string;
      readonly fieldName: string;
      readonly operationId: string;
      readonly status: string;
    }>;
    readonly outcome: string;
  };
};
export type mutationsResolveFormQuery = {
  response: mutationsResolveFormQuery$data;
  variables: mutationsResolveFormQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "operationId",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "requestId",
    },
    v3 = [
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
            name: "operationId",
            variableName: "operationId",
          },
          {
            kind: "Variable",
            name: "requestId",
            variableName: "requestId",
          },
        ],
        concreteType: "MetadataOperation",
        kind: "LinkedField",
        name: "metadataOperation",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "outcome",
            storageKey: null,
          },
          {
            alias: null,
            args: null,
            concreteType: "FieldOperation",
            kind: "LinkedField",
            name: "operations",
            plural: true,
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "operationId",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "fieldName",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "expiresAt",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "status",
                storageKey: null,
              },
            ],
            storageKey: null,
          },
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
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "manifestationId",
                storageKey: null,
              },
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
      name: "mutationsResolveFormQuery",
      selections: v3 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v0 /*:: as any*/, v2 /*:: as any*/, v1 /*:: as any*/],
      kind: "Operation",
      name: "mutationsResolveFormQuery",
      selections: v3 /*:: as any*/,
    },
    params: {
      id: "15e9cc9dca8d51c00953141bbb203ad50cc3c0dbc050843534702a1ad31e5e16",
      metadata: {},
      name: "mutationsResolveFormQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "f588b80fca779d68b574df1a2ee1484d";

export default node;

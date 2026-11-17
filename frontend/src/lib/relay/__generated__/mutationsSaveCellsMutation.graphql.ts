/**
 * @generated SignedSource<<2652c7e89af0d5e4143284afad6ef89c>>
 * @relayHash 0b46e2743d3c3b379a2f47fcf404ca96
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID e37ab8749e16e17a5f168eb4ab0b4327eb4c424e43f12e36fbc0970dfde06e42

import { ConcreteRequest } from "relay-runtime";
export type ContentRatingInput =
  | "ADULT"
  | "EVERYONE"
  | "EXPLICIT"
  | "MATURE"
  | "TEEN"
  | "%future added value";
export type MetadataPatch = {
  contentRating?: ContentRatingInput | null | undefined;
  contributors?: ContributorPatch | null | undefined;
  description?: string | null | undefined;
  genres?: ReadonlyArray<string> | null | undefined;
  isbn10?: string | null | undefined;
  isbn13?: string | null | undefined;
  language?: string | null | undefined;
  manifestationIdentifiers?: ReadonlyArray<IdentifierPatch> | null | undefined;
  moods?: ReadonlyArray<string> | null | undefined;
  pages?: number | null | undefined;
  publicationDate?: string | null | undefined;
  publisher?: string | null | undefined;
  subtitle?: string | null | undefined;
  tags?: ReadonlyArray<string> | null | undefined;
  title?: string | null | undefined;
  workIdentifiers?: ReadonlyArray<IdentifierPatch> | null | undefined;
};
export type ContributorPatch = {
  author?: ReadonlyArray<string> | null | undefined;
  editor?: ReadonlyArray<string> | null | undefined;
  translator?: ReadonlyArray<string> | null | undefined;
};
export type IdentifierPatch = {
  scheme: string;
  value?: string | null | undefined;
};
export type mutationsSaveCellsMutation$variables = {
  baseline?: string | null | undefined;
  id: string;
  patch: MetadataPatch;
  requestId?: string | null | undefined;
};
export type mutationsSaveCellsMutation$data = {
  readonly updateMetadata: {
    readonly metadata: {
      readonly etag: string;
      readonly isbn13: string | null | undefined;
      readonly manifestationId: string;
      readonly pages: number | null | undefined;
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
    readonly requestId: string | null | undefined;
  };
};
export type mutationsSaveCellsMutation = {
  response: mutationsSaveCellsMutation$data;
  variables: mutationsSaveCellsMutation$variables;
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
    v3 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "requestId",
    },
    v4 = [
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
          {
            kind: "Variable",
            name: "requestId",
            variableName: "requestId",
          },
        ],
        concreteType: "MetadataSave",
        kind: "LinkedField",
        name: "updateMetadata",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "requestId",
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
                name: "isbn13",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "pages",
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
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/, v3 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsSaveCellsMutation",
      selections: v4 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v3 /*:: as any*/, v2 /*:: as any*/],
      kind: "Operation",
      name: "mutationsSaveCellsMutation",
      selections: v4 /*:: as any*/,
    },
    params: {
      id: "e37ab8749e16e17a5f168eb4ab0b4327eb4c424e43f12e36fbc0970dfde06e42",
      metadata: {},
      name: "mutationsSaveCellsMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "d04c125c83d5855c73c30bfa30898f39";

export default node;

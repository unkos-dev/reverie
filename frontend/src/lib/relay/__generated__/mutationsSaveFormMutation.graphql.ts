/**
 * @generated SignedSource<<bf9f331078ccd7118ebc4af9b3554ea7>>
 * @relayHash 3971f8015822a25ab6f376bf7dc2651b
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 77c213ad9147cf705e9e8829edf6d2183c5b4fc187e92d3a61a8ffc85110a25b

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
export type mutationsSaveFormMutation$variables = {
  baseline?: string | null | undefined;
  id: string;
  patch: MetadataPatch;
  requestId?: string | null | undefined;
};
export type mutationsSaveFormMutation$data = {
  readonly updateMetadata: {
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
    readonly requestId: string | null | undefined;
  };
};
export type mutationsSaveFormMutation = {
  response: mutationsSaveFormMutation$data;
  variables: mutationsSaveFormMutation$variables;
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
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/, v3 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsSaveFormMutation",
      selections: v4 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v3 /*:: as any*/, v2 /*:: as any*/],
      kind: "Operation",
      name: "mutationsSaveFormMutation",
      selections: v4 /*:: as any*/,
    },
    params: {
      id: "77c213ad9147cf705e9e8829edf6d2183c5b4fc187e92d3a61a8ffc85110a25b",
      metadata: {},
      name: "mutationsSaveFormMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "c68a39efcd7038aa074beab60e64364d";

export default node;

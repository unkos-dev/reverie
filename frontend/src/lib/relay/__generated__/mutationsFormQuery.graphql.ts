/**
 * @generated SignedSource<<2b7cf01d3ef9b6e72926459974b9c303>>
 * @relayHash 6f50c94ab6276d10d79ffa34ed4bcf38
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID fe24a5219c040281081ec3d584c7e484f5defeee0dfbf62d9cb0fcdb5598e395

import { ConcreteRequest } from "relay-runtime";
export type mutationsFormQuery$variables = {
  id: string;
};
export type mutationsFormQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
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
      }
    | null
    | undefined;
};
export type mutationsFormQuery = {
  response: mutationsFormQuery$data;
  variables: mutationsFormQuery$variables;
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
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "id",
            variableName: "id",
          },
        ],
        concreteType: "Manifestation",
        kind: "LinkedField",
        name: "manifestation",
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
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsFormQuery",
      selections: v1 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsFormQuery",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "fe24a5219c040281081ec3d584c7e484f5defeee0dfbf62d9cb0fcdb5598e395",
      metadata: {},
      name: "mutationsFormQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "83e0b02f20f6e5ca89d45735d9d6b178";

export default node;

/**
 * @generated SignedSource<<d66de675ed07705669f7c0e7ed2854de>>
 * @relayHash 6d2010a9c67a8a3c94c433034cf14eaf
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 1c3656b0f549be2b655e627d9f91bc5cad56d8d92acf7c74f694b4278cb1b314

import { ConcreteRequest } from "relay-runtime";
export type mutationsResolveAuthorsQuery$variables = {
  id: string;
  operationId?: string | null | undefined;
  requestId?: string | null | undefined;
};
export type mutationsResolveAuthorsQuery$data = {
  readonly metadataOperation: {
    readonly metadata: {
      readonly authors: {
        readonly edges: ReadonlyArray<{
          readonly cursor: string;
          readonly node: {
            readonly id: string;
            readonly name: string;
          };
        }>;
        readonly pageInfo: {
          readonly endCursor: string | null | undefined;
          readonly hasNextPage: boolean;
          readonly hasPreviousPage: boolean;
          readonly startCursor: string | null | undefined;
        };
      };
      readonly etag: string;
      readonly manifestationId: string;
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
export type mutationsResolveAuthorsQuery = {
  response: mutationsResolveAuthorsQuery$data;
  variables: mutationsResolveAuthorsQuery$variables;
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
                args: [
                  {
                    kind: "Literal",
                    name: "first",
                    value: 20,
                  },
                ],
                concreteType: "AuthorConnection",
                kind: "LinkedField",
                name: "authors",
                plural: false,
                selections: [
                  {
                    alias: null,
                    args: null,
                    concreteType: "AuthorEdge",
                    kind: "LinkedField",
                    name: "edges",
                    plural: true,
                    selections: [
                      {
                        alias: null,
                        args: null,
                        kind: "ScalarField",
                        name: "cursor",
                        storageKey: null,
                      },
                      {
                        alias: null,
                        args: null,
                        concreteType: "Author",
                        kind: "LinkedField",
                        name: "node",
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
                            name: "name",
                            storageKey: null,
                          },
                        ],
                        storageKey: null,
                      },
                    ],
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    concreteType: "PageInfo",
                    kind: "LinkedField",
                    name: "pageInfo",
                    plural: false,
                    selections: [
                      {
                        alias: null,
                        args: null,
                        kind: "ScalarField",
                        name: "hasNextPage",
                        storageKey: null,
                      },
                      {
                        alias: null,
                        args: null,
                        kind: "ScalarField",
                        name: "hasPreviousPage",
                        storageKey: null,
                      },
                      {
                        alias: null,
                        args: null,
                        kind: "ScalarField",
                        name: "startCursor",
                        storageKey: null,
                      },
                      {
                        alias: null,
                        args: null,
                        kind: "ScalarField",
                        name: "endCursor",
                        storageKey: null,
                      },
                    ],
                    storageKey: null,
                  },
                ],
                storageKey: "authors(first:20)",
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
      name: "mutationsResolveAuthorsQuery",
      selections: v3 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v0 /*:: as any*/, v2 /*:: as any*/, v1 /*:: as any*/],
      kind: "Operation",
      name: "mutationsResolveAuthorsQuery",
      selections: v3 /*:: as any*/,
    },
    params: {
      id: "1c3656b0f549be2b655e627d9f91bc5cad56d8d92acf7c74f694b4278cb1b314",
      metadata: {},
      name: "mutationsResolveAuthorsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "847d1dd8860d2b3b4ba2791c89160ca4";

export default node;

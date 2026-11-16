/**
 * @generated SignedSource<<5ce7ab0b8b4be6f877c2ef4e5e0cea46>>
 * @relayHash f9fca8205f49c71750d7bf9d3a3551cb
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID ffa0174144dd3e54b36669bc9818b003034a3f53be68e9b3947e4ccfb672fe43

import { ConcreteRequest } from "relay-runtime";
export type mutationsUndoAuthorsMutation$variables = {
  baseline?: string | null | undefined;
  id: string;
  operationId: string;
};
export type mutationsUndoAuthorsMutation$data = {
  readonly undoMetadata: {
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
export type mutationsUndoAuthorsMutation = {
  response: mutationsUndoAuthorsMutation$data;
  variables: mutationsUndoAuthorsMutation$variables;
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
      name: "operationId",
    },
    v3 = [
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
            name: "operationId",
            variableName: "operationId",
          },
        ],
        concreteType: "MetadataOperation",
        kind: "LinkedField",
        name: "undoMetadata",
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
      name: "mutationsUndoAuthorsMutation",
      selections: v3 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v2 /*:: as any*/],
      kind: "Operation",
      name: "mutationsUndoAuthorsMutation",
      selections: v3 /*:: as any*/,
    },
    params: {
      id: "ffa0174144dd3e54b36669bc9818b003034a3f53be68e9b3947e4ccfb672fe43",
      metadata: {},
      name: "mutationsUndoAuthorsMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "5d43bebf141487bf5eb72023e4049114";

export default node;

/**
 * @generated SignedSource<<e0601b551819fb0ccd8168a20895275b>>
 * @relayHash 2ab1919b503cb4d7d9df93674ef1dbd1
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 5361854cda8108e3fd1889a145a165082b5a649362b430d78571c38555b3793f

import { ConcreteRequest } from "relay-runtime";
export type FieldOperationStatus =
  | "CONSUMED"
  | "ELIGIBLE"
  | "LOCKED"
  | "UNAVAILABLE"
  | "%future added value";
export type OperationOutcome = "RESTORED" | "SAVED" | "UNKNOWN" | "%future added value";
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
            readonly author: {
              readonly id: string;
            };
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
      readonly status: FieldOperationStatus;
    }>;
    readonly outcome: OperationOutcome;
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
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "outcome",
      storageKey: null,
    },
    v5 = {
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
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "workId",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "manifestationId",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v10 = {
      alias: null,
      args: [
        {
          kind: "Literal",
          name: "first",
          value: 20,
        },
      ],
      concreteType: "RevisionContributorConnection",
      kind: "LinkedField",
      name: "authors",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "RevisionContributorEdge",
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
              concreteType: "RevisionContributor",
              kind: "LinkedField",
              name: "node",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "name",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  concreteType: "Author",
                  kind: "LinkedField",
                  name: "author",
                  plural: false,
                  selections: [v9 /*:: as any*/],
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
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsResolveAuthorsQuery",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "MetadataOperation",
          kind: "LinkedField",
          name: "metadataOperation",
          plural: false,
          selections: [
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [v6 /*:: as any*/, v7 /*:: as any*/, v8 /*:: as any*/, v10 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v0 /*:: as any*/, v2 /*:: as any*/, v1 /*:: as any*/],
      kind: "Operation",
      name: "mutationsResolveAuthorsQuery",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "MetadataOperation",
          kind: "LinkedField",
          name: "metadataOperation",
          plural: false,
          selections: [
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [
                v6 /*:: as any*/,
                v7 /*:: as any*/,
                v8 /*:: as any*/,
                v10 /*:: as any*/,
                v9 /*:: as any*/,
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "5361854cda8108e3fd1889a145a165082b5a649362b430d78571c38555b3793f",
      metadata: {},
      name: "mutationsResolveAuthorsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "87d16695b0f4689304c713d7b6d94f6e";

export default node;

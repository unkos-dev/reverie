/**
 * @generated SignedSource<<ce0886c3ffaf847e7fb2aa5da42aa60a>>
 * @relayHash 9b4684081a84eeb8ded193e5cb784596
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 8289c404b72545bc2de0dec6e5f6d61398c049889056c22167d760266bfdd4ac

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
      name: "mutationsUndoAuthorsMutation",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "MetadataOperation",
          kind: "LinkedField",
          name: "undoMetadata",
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
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v2 /*:: as any*/],
      kind: "Operation",
      name: "mutationsUndoAuthorsMutation",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "MetadataOperation",
          kind: "LinkedField",
          name: "undoMetadata",
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
      id: "8289c404b72545bc2de0dec6e5f6d61398c049889056c22167d760266bfdd4ac",
      metadata: {},
      name: "mutationsUndoAuthorsMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "cadfe6fe90f26df1d9d0490dae18613a";

export default node;

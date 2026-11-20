/**
 * @generated SignedSource<<6875eec608e89ac3ce1a138c50ec5db7>>
 * @relayHash 90a4836ccc3db43dcb371855769b9705
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 73b43926dc5b6ca5ce2523434974e2a7054eaebe51262bbf165e4d47a5c5740e

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
export type mutationsSaveAuthorsMutation$variables = {
  baseline?: string | null | undefined;
  id: string;
  patch: MetadataPatch;
  requestId?: string | null | undefined;
};
export type mutationsSaveAuthorsMutation$data = {
  readonly updateMetadata: {
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
    readonly requestId: string | null | undefined;
  };
};
export type mutationsSaveAuthorsMutation = {
  response: mutationsSaveAuthorsMutation$data;
  variables: mutationsSaveAuthorsMutation$variables;
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
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "requestId",
      storageKey: null,
    },
    v6 = {
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
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "workId",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "manifestationId",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v11 = {
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
                  selections: [v10 /*:: as any*/],
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
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/, v3 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsSaveAuthorsMutation",
      selections: [
        {
          alias: null,
          args: v4 /*:: as any*/,
          concreteType: "MetadataSave",
          kind: "LinkedField",
          name: "updateMetadata",
          plural: false,
          selections: [
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [v7 /*:: as any*/, v8 /*:: as any*/, v9 /*:: as any*/, v11 /*:: as any*/],
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
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/, v3 /*:: as any*/, v2 /*:: as any*/],
      kind: "Operation",
      name: "mutationsSaveAuthorsMutation",
      selections: [
        {
          alias: null,
          args: v4 /*:: as any*/,
          concreteType: "MetadataSave",
          kind: "LinkedField",
          name: "updateMetadata",
          plural: false,
          selections: [
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [
                v7 /*:: as any*/,
                v8 /*:: as any*/,
                v9 /*:: as any*/,
                v11 /*:: as any*/,
                v10 /*:: as any*/,
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "73b43926dc5b6ca5ce2523434974e2a7054eaebe51262bbf165e4d47a5c5740e",
      metadata: {},
      name: "mutationsSaveAuthorsMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "1edbfbbe986d4712bd6bb73ba4358723";

export default node;

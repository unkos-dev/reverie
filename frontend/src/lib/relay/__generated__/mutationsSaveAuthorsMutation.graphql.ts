/**
 * @generated SignedSource<<8db4c6a428046cdb7f383e5370eb8d75>>
 * @relayHash 909a67e066988126e711f99f690c0758
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID c6c2fbee55578d3e3a3d9f41293559ff58784e9e95dfa5edc21010c3ea48eb98

import { ConcreteRequest } from "relay-runtime";
export type ContentRatingInput =
  | "ADULT"
  | "EVERYONE"
  | "EXPLICIT"
  | "MATURE"
  | "TEEN"
  | "%future added value";
export type FieldOperationStatus =
  | "CONSUMED"
  | "ELIGIBLE"
  | "LOCKED"
  | "UNAVAILABLE"
  | "%future added value";
export type UpdateMetadataInput = {
  baseline: string;
  id: string;
  patch: MetadataPatch;
  requestId?: string | null | undefined;
};
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
  input: UpdateMetadataInput;
};
export type mutationsSaveAuthorsMutation$data = {
  readonly updateMetadata:
    | {
        readonly __typename: "EditTooLarge";
        readonly message: string;
      }
    | {
        readonly __typename: "FieldsLocked";
        readonly fields: ReadonlyArray<string>;
        readonly message: string;
      }
    | {
        readonly __typename: "MetadataInvalid";
        readonly fieldErrors: ReadonlyArray<{
          readonly field: string;
          readonly message: string;
        }>;
        readonly message: string;
      }
    | {
        readonly __typename: "MetadataSaved";
        readonly manifestation: {
          readonly id: string;
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
        };
        readonly operations: ReadonlyArray<{
          readonly expiresAt: string;
          readonly fieldName: string;
          readonly operationId: string;
          readonly status: FieldOperationStatus;
        }>;
        readonly requestId: string | null | undefined;
      }
    | {
        readonly __typename: "NotFound";
        readonly message: string;
      }
    | {
        readonly __typename: "StaleBaseline";
        readonly message: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type mutationsSaveAuthorsMutation = {
  response: mutationsSaveAuthorsMutation$data;
  variables: mutationsSaveAuthorsMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "input",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "input",
        variableName: "input",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "requestId",
      storageKey: null,
    },
    v4 = {
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
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
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
                  selections: [v5 /*:: as any*/],
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
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "message",
      storageKey: null,
    },
    v11 = [v10 /*:: as any*/],
    v12 = {
      kind: "InlineFragment",
      selections: v11 /*:: as any*/,
      type: "StaleBaseline",
      abstractKey: null,
    },
    v13 = {
      kind: "InlineFragment",
      selections: [
        v10 /*:: as any*/,
        {
          alias: null,
          args: null,
          concreteType: "FieldError",
          kind: "LinkedField",
          name: "fieldErrors",
          plural: true,
          selections: [
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "field",
              storageKey: null,
            },
            v10 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
      type: "MetadataInvalid",
      abstractKey: null,
    },
    v14 = {
      kind: "InlineFragment",
      selections: [
        v10 /*:: as any*/,
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "fields",
          storageKey: null,
        },
      ],
      type: "FieldsLocked",
      abstractKey: null,
    },
    v15 = {
      kind: "InlineFragment",
      selections: v11 /*:: as any*/,
      type: "EditTooLarge",
      abstractKey: null,
    },
    v16 = {
      kind: "InlineFragment",
      selections: v11 /*:: as any*/,
      type: "NotFound",
      abstractKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsSaveAuthorsMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "updateMetadata",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                v3 /*:: as any*/,
                v4 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Manifestation",
                  kind: "LinkedField",
                  name: "manifestation",
                  plural: false,
                  selections: [
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
                        v9 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataSaved",
              abstractKey: null,
            },
            v12 /*:: as any*/,
            v13 /*:: as any*/,
            v14 /*:: as any*/,
            v15 /*:: as any*/,
            v16 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsSaveAuthorsMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "updateMetadata",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                v3 /*:: as any*/,
                v4 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Manifestation",
                  kind: "LinkedField",
                  name: "manifestation",
                  plural: false,
                  selections: [
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
                        v9 /*:: as any*/,
                        v5 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataSaved",
              abstractKey: null,
            },
            {
              kind: "TypeDiscriminator",
              abstractKey: "__isUpdateMetadataResult",
            },
            v12 /*:: as any*/,
            v13 /*:: as any*/,
            v14 /*:: as any*/,
            v15 /*:: as any*/,
            v16 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "c6c2fbee55578d3e3a3d9f41293559ff58784e9e95dfa5edc21010c3ea48eb98",
      metadata: {},
      name: "mutationsSaveAuthorsMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "816e5f364bb46a4d48dbdfefce705a17";

export default node;

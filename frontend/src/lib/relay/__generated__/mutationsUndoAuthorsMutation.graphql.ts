/**
 * @generated SignedSource<<70f963af01bbcb99fb660cdad115485a>>
 * @relayHash 6e4cd4fc22af7e430affe71c860c0c88
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID af779ce69b3366ce438ed21698a69431ab6575ad22185b662bd3fbbe97b937d8

import { ConcreteRequest } from "relay-runtime";
export type FieldOperationStatus =
  | "CONSUMED"
  | "ELIGIBLE"
  | "LOCKED"
  | "UNAVAILABLE"
  | "%future added value";
export type UndoMetadataInput = {
  baseline: string;
  id: string;
  operationId: string;
};
export type mutationsUndoAuthorsMutation$variables = {
  input: UndoMetadataInput;
};
export type mutationsUndoAuthorsMutation$data = {
  readonly undoMetadata:
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
        readonly __typename: "MetadataRestored";
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
        readonly operation: {
          readonly expiresAt: string;
          readonly fieldName: string;
          readonly operationId: string;
          readonly status: FieldOperationStatus;
        };
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
        readonly __typename: "UndoConflict";
        readonly message: string;
      }
    | {
        readonly __typename: "UndoUnavailable";
        readonly message: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type mutationsUndoAuthorsMutation = {
  response: mutationsUndoAuthorsMutation$data;
  variables: mutationsUndoAuthorsMutation$variables;
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
      concreteType: "FieldOperation",
      kind: "LinkedField",
      name: "operation",
      plural: false,
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
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "workId",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "manifestationId",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v8 = {
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
                  selections: [v4 /*:: as any*/],
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
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "message",
      storageKey: null,
    },
    v10 = [v9 /*:: as any*/],
    v11 = {
      kind: "InlineFragment",
      selections: v10 /*:: as any*/,
      type: "StaleBaseline",
      abstractKey: null,
    },
    v12 = {
      kind: "InlineFragment",
      selections: v10 /*:: as any*/,
      type: "UndoUnavailable",
      abstractKey: null,
    },
    v13 = {
      kind: "InlineFragment",
      selections: v10 /*:: as any*/,
      type: "UndoConflict",
      abstractKey: null,
    },
    v14 = {
      kind: "InlineFragment",
      selections: [
        v9 /*:: as any*/,
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
      selections: v10 /*:: as any*/,
      type: "EditTooLarge",
      abstractKey: null,
    },
    v16 = {
      kind: "InlineFragment",
      selections: v10 /*:: as any*/,
      type: "NotFound",
      abstractKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsUndoAuthorsMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "undoMetadata",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                v3 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Manifestation",
                  kind: "LinkedField",
                  name: "manifestation",
                  plural: false,
                  selections: [
                    v4 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      concreteType: "MetadataRevision",
                      kind: "LinkedField",
                      name: "metadata",
                      plural: false,
                      selections: [
                        v5 /*:: as any*/,
                        v6 /*:: as any*/,
                        v7 /*:: as any*/,
                        v8 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataRestored",
              abstractKey: null,
            },
            v11 /*:: as any*/,
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
      name: "mutationsUndoAuthorsMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "undoMetadata",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                v3 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Manifestation",
                  kind: "LinkedField",
                  name: "manifestation",
                  plural: false,
                  selections: [
                    v4 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      concreteType: "MetadataRevision",
                      kind: "LinkedField",
                      name: "metadata",
                      plural: false,
                      selections: [
                        v5 /*:: as any*/,
                        v6 /*:: as any*/,
                        v7 /*:: as any*/,
                        v8 /*:: as any*/,
                        v4 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataRestored",
              abstractKey: null,
            },
            {
              kind: "TypeDiscriminator",
              abstractKey: "__isUndoMetadataResult",
            },
            v11 /*:: as any*/,
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
      id: "af779ce69b3366ce438ed21698a69431ab6575ad22185b662bd3fbbe97b937d8",
      metadata: {},
      name: "mutationsUndoAuthorsMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "6d05ff93351296b41300d0f5cd3c10a7";

export default node;

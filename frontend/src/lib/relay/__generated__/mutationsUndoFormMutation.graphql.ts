/**
 * @generated SignedSource<<69c10e813e29eeaf6a07f497606d77c7>>
 * @relayHash 5c40552f85da3c49795c71cef38f2b24
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID bf8278a1dd7d67d778118c76e89d5737cc3d2bfbefd4d1049ba00de15f2d2738

import { ConcreteRequest } from "relay-runtime";
export type UndoMetadataInput = {
  baseline: string;
  id: string;
  operationId: string;
};
export type mutationsUndoFormMutation$variables = {
  input: UndoMetadataInput;
};
export type mutationsUndoFormMutation$data = {
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
            readonly contentRating: string | null | undefined;
            readonly description: string | null | undefined;
            readonly etag: string;
            readonly id: string;
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
        };
        readonly operation: {
          readonly expiresAt: string;
          readonly fieldName: string;
          readonly operationId: string;
          readonly status: string;
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
export type mutationsUndoFormMutation = {
  response: mutationsUndoFormMutation$data;
  variables: mutationsUndoFormMutation$variables;
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
      name: "id",
      storageKey: null,
    },
    v4 = {
      kind: "InlineFragment",
      selections: [
        {
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
        {
          alias: null,
          args: null,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [
                v3 /*:: as any*/,
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
      ],
      type: "MetadataRestored",
      abstractKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "message",
      storageKey: null,
    },
    v6 = [v5 /*:: as any*/],
    v7 = {
      kind: "InlineFragment",
      selections: v6 /*:: as any*/,
      type: "StaleBaseline",
      abstractKey: null,
    },
    v8 = {
      kind: "InlineFragment",
      selections: v6 /*:: as any*/,
      type: "UndoUnavailable",
      abstractKey: null,
    },
    v9 = {
      kind: "InlineFragment",
      selections: v6 /*:: as any*/,
      type: "UndoConflict",
      abstractKey: null,
    },
    v10 = {
      kind: "InlineFragment",
      selections: [
        v5 /*:: as any*/,
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
    v11 = {
      kind: "InlineFragment",
      selections: v6 /*:: as any*/,
      type: "EditTooLarge",
      abstractKey: null,
    },
    v12 = {
      kind: "InlineFragment",
      selections: v6 /*:: as any*/,
      type: "NotFound",
      abstractKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsUndoFormMutation",
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
            v4 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
            v10 /*:: as any*/,
            v11 /*:: as any*/,
            v12 /*:: as any*/,
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
      name: "mutationsUndoFormMutation",
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
            v4 /*:: as any*/,
            {
              kind: "TypeDiscriminator",
              abstractKey: "__isUndoMetadataResult",
            },
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
            v10 /*:: as any*/,
            v11 /*:: as any*/,
            v12 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "bf8278a1dd7d67d778118c76e89d5737cc3d2bfbefd4d1049ba00de15f2d2738",
      metadata: {},
      name: "mutationsUndoFormMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "9733276e771a1b69ae02cd9b9920f91e";

export default node;

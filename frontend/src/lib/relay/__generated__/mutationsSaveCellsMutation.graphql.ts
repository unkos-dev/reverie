/**
 * @generated SignedSource<<d0341d6d8dd7a162ce8a14ba5cd34db3>>
 * @relayHash 7376a362d7924181c1345a76e518a9df
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 0467e640d6dce22d9e5de921d8ab1f7802da531f99c7ae69bacefb1bd4f633cb

import { ConcreteRequest } from "relay-runtime";
export type ContentRatingInput =
  | "ADULT"
  | "EVERYONE"
  | "EXPLICIT"
  | "MATURE"
  | "TEEN"
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
export type mutationsSaveCellsMutation$variables = {
  input: UpdateMetadataInput;
};
export type mutationsSaveCellsMutation$data = {
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
            readonly etag: string;
            readonly id: string;
            readonly isbn13: string | null | undefined;
            readonly manifestationId: string;
            readonly pages: number | null | undefined;
            readonly subtitle: string | null | undefined;
            readonly title: string;
            readonly workId: string;
          };
        };
        readonly operations: ReadonlyArray<{
          readonly expiresAt: string;
          readonly fieldName: string;
          readonly operationId: string;
          readonly status: string;
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
export type mutationsSaveCellsMutation = {
  response: mutationsSaveCellsMutation$data;
  variables: mutationsSaveCellsMutation$variables;
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
                  name: "isbn13",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "pages",
                  storageKey: null,
                },
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
      selections: [
        v5 /*:: as any*/,
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
            v5 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
      type: "MetadataInvalid",
      abstractKey: null,
    },
    v9 = {
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
    v10 = {
      kind: "InlineFragment",
      selections: v6 /*:: as any*/,
      type: "EditTooLarge",
      abstractKey: null,
    },
    v11 = {
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
      name: "mutationsSaveCellsMutation",
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
            v4 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
            v10 /*:: as any*/,
            v11 /*:: as any*/,
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
      name: "mutationsSaveCellsMutation",
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
            v4 /*:: as any*/,
            {
              kind: "TypeDiscriminator",
              abstractKey: "__isUpdateMetadataResult",
            },
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
            v10 /*:: as any*/,
            v11 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "0467e640d6dce22d9e5de921d8ab1f7802da531f99c7ae69bacefb1bd4f633cb",
      metadata: {},
      name: "mutationsSaveCellsMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "11ea6e8634b36faaf98ab9b52f0b8c19";

export default node;

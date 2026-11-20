/**
 * @generated SignedSource<<e5066d9b19537d0ce22ee28406a5eeb6>>
 * @relayHash 0313523f00546d09cffb347be0e16fbd
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 2849120ec3583b88ee934e26845b5b34464a5798c4fd24d0176d85c6cf41699b

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
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
export type mutationsSaveFormMutation$variables = {
  input: UpdateMetadataInput;
};
export type mutationsSaveFormMutation$data = {
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
            readonly " $fragmentSpreads": FragmentRefs<"mutationsFormSnapshot">;
          };
          readonly " $fragmentSpreads": FragmentRefs<"readerDetailEditable">;
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
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type mutationsSaveFormMutation = {
  response: mutationsSaveFormMutation$data;
  variables: mutationsSaveFormMutation$variables;
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
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "message",
      storageKey: null,
    },
    v5 = [v4 /*:: as any*/],
    v6 = {
      kind: "InlineFragment",
      selections: v5 /*:: as any*/,
      type: "StaleBaseline",
      abstractKey: null,
    },
    v7 = {
      kind: "InlineFragment",
      selections: [
        v4 /*:: as any*/,
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
            v4 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
      type: "MetadataInvalid",
      abstractKey: null,
    },
    v8 = {
      kind: "InlineFragment",
      selections: [
        v4 /*:: as any*/,
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
    v9 = {
      kind: "InlineFragment",
      selections: v5 /*:: as any*/,
      type: "EditTooLarge",
      abstractKey: null,
    },
    v10 = {
      kind: "InlineFragment",
      selections: v5 /*:: as any*/,
      type: "NotFound",
      abstractKey: null,
    },
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v12 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "description",
      storageKey: null,
    },
    v13 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "language",
      storageKey: null,
    },
    v14 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v15 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v16 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
      storageKey: null,
    },
    v17 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v18 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbnMismatch",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsSaveFormMutation",
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
                        {
                          args: null,
                          kind: "FragmentSpread",
                          name: "mutationsFormSnapshot",
                        },
                      ],
                      storageKey: null,
                    },
                    {
                      args: null,
                      kind: "FragmentSpread",
                      name: "readerDetailEditable",
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataSaved",
              abstractKey: null,
            },
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
            v10 /*:: as any*/,
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
      name: "mutationsSaveFormMutation",
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
                        v11 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "subtitle",
                          storageKey: null,
                        },
                        v12 /*:: as any*/,
                        v13 /*:: as any*/,
                        v14 /*:: as any*/,
                        v15 /*:: as any*/,
                        v16 /*:: as any*/,
                        v17 /*:: as any*/,
                        v18 /*:: as any*/,
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
                    v17 /*:: as any*/,
                    v16 /*:: as any*/,
                    v18 /*:: as any*/,
                    v14 /*:: as any*/,
                    v15 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "work",
                      plural: false,
                      selections: [
                        v3 /*:: as any*/,
                        v11 /*:: as any*/,
                        v12 /*:: as any*/,
                        v13 /*:: as any*/,
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
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
            v10 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "2849120ec3583b88ee934e26845b5b34464a5798c4fd24d0176d85c6cf41699b",
      metadata: {},
      name: "mutationsSaveFormMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "069347e5945e13613e5f9ddf16ec37b6";

export default node;

/**
 * @generated SignedSource<<5b8645775bbeab7c748fbd8f60a4bdc4>>
 * @relayHash 9ffd9ee5b2038ff06617455896257a59
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 390de682c17d7e78510caf8cd09cd15d6cc3cabd0f5dc14d7b9022532c526947

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
export type useCellEditSaveMutation$variables = {
  input: UpdateMetadataInput;
};
export type useCellEditSaveMutation$data = {
  readonly updateMetadata:
    | {
        readonly __typename: "FieldsLocked";
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
          readonly " $fragmentSpreads": FragmentRefs<"BookCard_book" | "LibraryTable_cell">;
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
export type useCellEditSaveMutation = {
  response: useCellEditSaveMutation$data;
  variables: useCellEditSaveMutation$variables;
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
      name: "message",
      storageKey: null,
    },
    v4 = [v3 /*:: as any*/],
    v5 = {
      kind: "InlineFragment",
      selections: v4 /*:: as any*/,
      type: "StaleBaseline",
      abstractKey: null,
    },
    v6 = {
      kind: "InlineFragment",
      selections: [
        v3 /*:: as any*/,
        {
          alias: null,
          args: null,
          concreteType: "FieldError",
          kind: "LinkedField",
          name: "fieldErrors",
          plural: true,
          selections: [
            v3 /*:: as any*/,
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "field",
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
      type: "MetadataInvalid",
      abstractKey: null,
    },
    v7 = {
      kind: "InlineFragment",
      selections: v4 /*:: as any*/,
      type: "FieldsLocked",
      abstractKey: null,
    },
    v8 = {
      kind: "InlineFragment",
      selections: v4 /*:: as any*/,
      type: "NotFound",
      abstractKey: null,
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
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v12 = [
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "name",
        storageKey: null,
      },
      v9 /*:: as any*/,
    ],
    v13 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "useCellEditSaveMutation",
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
                    {
                      args: null,
                      kind: "FragmentSpread",
                      name: "LibraryTable_cell",
                    },
                    {
                      args: null,
                      kind: "FragmentSpread",
                      name: "BookCard_book",
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataSaved",
              abstractKey: null,
            },
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
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
      name: "useCellEditSaveMutation",
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
                    v9 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "coverUrl",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "createdAt",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "work",
                      plural: false,
                      selections: [
                        v9 /*:: as any*/,
                        v10 /*:: as any*/,
                        v11 /*:: as any*/,
                        {
                          alias: null,
                          args: [
                            {
                              kind: "Literal",
                              name: "first",
                              value: 5,
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
                                  concreteType: "Author",
                                  kind: "LinkedField",
                                  name: "node",
                                  plural: false,
                                  selections: v12 /*:: as any*/,
                                  storageKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                          ],
                          storageKey: "authors(first:5)",
                        },
                        {
                          alias: null,
                          args: null,
                          concreteType: "SelectedSeries",
                          kind: "LinkedField",
                          name: "selectedSeries",
                          plural: false,
                          selections: [
                            {
                              alias: null,
                              args: null,
                              concreteType: "Series",
                              kind: "LinkedField",
                              name: "node",
                              plural: false,
                              selections: v12 /*:: as any*/,
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "position",
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
                      concreteType: "MetadataRevision",
                      kind: "LinkedField",
                      name: "metadata",
                      plural: false,
                      selections: [
                        v13 /*:: as any*/,
                        v10 /*:: as any*/,
                        v11 /*:: as any*/,
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
                        v9 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      concreteType: "ReadingRevision",
                      kind: "LinkedField",
                      name: "readingState",
                      plural: false,
                      selections: [
                        v13 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "status",
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "rating",
                          storageKey: null,
                        },
                        v9 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "progressPct",
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
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "390de682c17d7e78510caf8cd09cd15d6cc3cabd0f5dc14d7b9022532c526947",
      metadata: {},
      name: "useCellEditSaveMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "26147b94347762eff3b4fe1155f494db";

export default node;

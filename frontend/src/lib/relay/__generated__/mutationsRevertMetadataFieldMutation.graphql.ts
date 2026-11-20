/**
 * @generated SignedSource<<a0bae8543d44002088343d0a2acd660b>>
 * @relayHash 75c1bb257012308a69a0dfe2dee9b449
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 5160b3098871c7e86a0a314c9ec08070a4fa219b3d97a372375ed32420998fe1

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type RevertMetadataFieldInput = {
  fieldName: string;
  id: string;
  versionId?: string | null | undefined;
};
export type mutationsRevertMetadataFieldMutation$variables = {
  input: RevertMetadataFieldInput;
};
export type mutationsRevertMetadataFieldMutation$data = {
  readonly revertMetadataField:
    | {
        readonly __typename: "FieldsLocked";
        readonly fields: ReadonlyArray<string>;
        readonly message: string;
      }
    | {
        readonly __typename: "MetadataFieldReverted";
        readonly manifestation: {
          readonly id: string;
          readonly metadata: {
            readonly " $fragmentSpreads": FragmentRefs<"mutationsFormSnapshot">;
          };
          readonly " $fragmentSpreads": FragmentRefs<"readerDetailBook" | "readerVersions">;
        };
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
        readonly __typename: "NotFound";
        readonly message: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type mutationsRevertMetadataFieldMutation = {
  response: mutationsRevertMetadataFieldMutation$data;
  variables: mutationsRevertMetadataFieldMutation$variables;
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
    v5 = {
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
    v6 = {
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
    v7 = {
      kind: "InlineFragment",
      selections: [v4 /*:: as any*/],
      type: "NotFound",
      abstractKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "description",
      storageKey: null,
    },
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "language",
      storageKey: null,
    },
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v12 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v13 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
      storageKey: null,
    },
    v14 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v15 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbnMismatch",
      storageKey: null,
    },
    v16 = [
      {
        kind: "Literal",
        name: "first",
        value: 20,
      },
    ],
    v17 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v18 = [v3 /*:: as any*/, v17 /*:: as any*/, v2 /*:: as any*/],
    v19 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v20 = {
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
          name: "endCursor",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "hasNextPage",
          storageKey: null,
        },
      ],
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsRevertMetadataFieldMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "revertMetadataField",
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
                      name: "readerDetailBook",
                    },
                    {
                      args: null,
                      kind: "FragmentSpread",
                      name: "readerVersions",
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataFieldReverted",
              abstractKey: null,
            },
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
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
      name: "mutationsRevertMetadataFieldMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "revertMetadataField",
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
                        v8 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "subtitle",
                          storageKey: null,
                        },
                        v9 /*:: as any*/,
                        v10 /*:: as any*/,
                        v11 /*:: as any*/,
                        v12 /*:: as any*/,
                        v13 /*:: as any*/,
                        v14 /*:: as any*/,
                        v15 /*:: as any*/,
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
                    v14 /*:: as any*/,
                    v13 /*:: as any*/,
                    v15 /*:: as any*/,
                    v11 /*:: as any*/,
                    v12 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "work",
                      plural: false,
                      selections: [
                        v3 /*:: as any*/,
                        v8 /*:: as any*/,
                        v9 /*:: as any*/,
                        v10 /*:: as any*/,
                        {
                          alias: null,
                          args: v16 /*:: as any*/,
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
                                  selections: v18 /*:: as any*/,
                                  storageKey: null,
                                },
                                v19 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v20 /*:: as any*/,
                          ],
                          storageKey: "authors(first:20)",
                        },
                        {
                          alias: null,
                          args: v16 /*:: as any*/,
                          filters: null,
                          handle: "connection",
                          key: "readerWorkAuthors_authors",
                          kind: "LinkedHandle",
                          name: "authors",
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
                              selections: [v3 /*:: as any*/, v17 /*:: as any*/],
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
                      kind: "ScalarField",
                      name: "coverUrl",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "ingestionStatus",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "validationStatus",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "enrichmentStatus",
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
                      concreteType: "MetadataVersions",
                      kind: "LinkedField",
                      name: "versions",
                      plural: false,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "pendingCount",
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: v16 /*:: as any*/,
                          concreteType: "MetadataVersionConnection",
                          kind: "LinkedField",
                          name: "pending",
                          plural: false,
                          selections: [
                            {
                              alias: null,
                              args: null,
                              concreteType: "MetadataVersionEdge",
                              kind: "LinkedField",
                              name: "edges",
                              plural: true,
                              selections: [
                                {
                                  alias: null,
                                  args: null,
                                  concreteType: "MetadataVersion",
                                  kind: "LinkedField",
                                  name: "node",
                                  plural: false,
                                  selections: [
                                    v3 /*:: as any*/,
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
                                      name: "source",
                                      storageKey: null,
                                    },
                                    {
                                      alias: null,
                                      args: null,
                                      kind: "ScalarField",
                                      name: "newValue",
                                      storageKey: null,
                                    },
                                    {
                                      alias: null,
                                      args: null,
                                      kind: "ScalarField",
                                      name: "confidenceScore",
                                      storageKey: null,
                                    },
                                    {
                                      alias: null,
                                      args: null,
                                      kind: "ScalarField",
                                      name: "matchType",
                                      storageKey: null,
                                    },
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
                                      name: "observationCount",
                                      storageKey: null,
                                    },
                                    v2 /*:: as any*/,
                                  ],
                                  storageKey: null,
                                },
                                v19 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v20 /*:: as any*/,
                          ],
                          storageKey: "pending(first:20)",
                        },
                        {
                          alias: null,
                          args: v16 /*:: as any*/,
                          filters: null,
                          handle: "connection",
                          key: "readerVersions_pending",
                          kind: "LinkedHandle",
                          name: "pending",
                        },
                      ],
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: v16 /*:: as any*/,
                      concreteType: "TagConnection",
                      kind: "LinkedField",
                      name: "tags",
                      plural: false,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          concreteType: "TagEdge",
                          kind: "LinkedField",
                          name: "edges",
                          plural: true,
                          selections: [
                            {
                              alias: null,
                              args: null,
                              concreteType: "Tag",
                              kind: "LinkedField",
                              name: "node",
                              plural: false,
                              selections: v18 /*:: as any*/,
                              storageKey: null,
                            },
                            v19 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                        v20 /*:: as any*/,
                      ],
                      storageKey: "tags(first:20)",
                    },
                    {
                      alias: null,
                      args: v16 /*:: as any*/,
                      filters: null,
                      handle: "connection",
                      key: "readerDetailTags_tags",
                      kind: "LinkedHandle",
                      name: "tags",
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataFieldReverted",
              abstractKey: null,
            },
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "5160b3098871c7e86a0a314c9ec08070a4fa219b3d97a372375ed32420998fe1",
      metadata: {},
      name: "mutationsRevertMetadataFieldMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "ced266effa095e3fa256867813d8b0ae";

export default node;

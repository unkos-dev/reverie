/**
 * @generated SignedSource<<fceae88a2cfcc12c2ecfafe152ee04da>>
 * @relayHash 8c4d89c20c2efde90fc09bf017b13dd6
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID c932e91fdd08d884e296e8afde9eca858d8052b4e266ad0a4004dcdb50e53c3f

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
    v8 = [
      {
        kind: "Literal",
        name: "first",
        value: 20,
      },
    ],
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v10 = [v3 /*:: as any*/, v9 /*:: as any*/, v2 /*:: as any*/],
    v11 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v12 = {
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
                      kind: "ScalarField",
                      name: "isbn13",
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
                      name: "isbnMismatch",
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
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "work",
                      plural: false,
                      selections: [
                        v3 /*:: as any*/,
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
                          args: v8 /*:: as any*/,
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
                                  selections: v10 /*:: as any*/,
                                  storageKey: null,
                                },
                                v11 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v12 /*:: as any*/,
                          ],
                          storageKey: "authors(first:20)",
                        },
                        {
                          alias: null,
                          args: v8 /*:: as any*/,
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
                              selections: [v3 /*:: as any*/, v9 /*:: as any*/],
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
                          args: v8 /*:: as any*/,
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
                                v11 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v12 /*:: as any*/,
                          ],
                          storageKey: "pending(first:20)",
                        },
                        {
                          alias: null,
                          args: v8 /*:: as any*/,
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
                      args: v8 /*:: as any*/,
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
                              selections: v10 /*:: as any*/,
                              storageKey: null,
                            },
                            v11 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                        v12 /*:: as any*/,
                      ],
                      storageKey: "tags(first:20)",
                    },
                    {
                      alias: null,
                      args: v8 /*:: as any*/,
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
      id: "c932e91fdd08d884e296e8afde9eca858d8052b4e266ad0a4004dcdb50e53c3f",
      metadata: {},
      name: "mutationsRevertMetadataFieldMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "0ed10220edc555666e2c30a045376ee1";

export default node;

/**
 * @generated SignedSource<<8a2049fe8d46159dba59e5f318a4a815>>
 * @relayHash 7e8be57d88d2a672c5a6c3b30cf896bc
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 6fa05c47ee955fb36eab7fb213e3fe3781e613b723287c2b683929c5095f552e

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type AcceptMetadataVersionInput = {
  id: string;
  versionId: string;
};
export type VersionsTabAcceptMutation$variables = {
  input: AcceptMetadataVersionInput;
};
export type VersionsTabAcceptMutation$data = {
  readonly acceptMetadataVersion:
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
        readonly __typename: "MetadataVersionAccepted";
        readonly manifestation: {
          readonly id: string;
          readonly metadata: {
            readonly " $fragmentSpreads": FragmentRefs<"EditMetadataDialog_opening">;
          };
          readonly work:
            | {
                readonly id: string;
              }
            | null
            | undefined;
          readonly " $fragmentSpreads": FragmentRefs<
            "BookPage_book" | "LibraryTable_cell" | "VersionsTab_proposals"
          >;
        };
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
export type VersionsTabAcceptMutation = {
  response: VersionsTabAcceptMutation$data;
  variables: VersionsTabAcceptMutation$variables;
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
      name: "subtitle",
      storageKey: null,
    },
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v11 = [v10 /*:: as any*/, v3 /*:: as any*/],
    v12 = [
      {
        kind: "Literal",
        name: "first",
        value: 20,
      },
    ],
    v13 = [v3 /*:: as any*/, v10 /*:: as any*/, v2 /*:: as any*/],
    v14 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v15 = {
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
    },
    v16 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "description",
      storageKey: null,
    },
    v17 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "language",
      storageKey: null,
    },
    v18 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v19 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v20 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v21 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn10",
      storageKey: null,
    },
    v22 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v23 = {
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
      name: "VersionsTabAcceptMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "acceptMetadataVersion",
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
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "work",
                      plural: false,
                      selections: [v3 /*:: as any*/],
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
                        {
                          args: null,
                          kind: "FragmentSpread",
                          name: "EditMetadataDialog_opening",
                        },
                      ],
                      storageKey: null,
                    },
                    {
                      args: null,
                      kind: "FragmentSpread",
                      name: "LibraryTable_cell",
                    },
                    {
                      args: null,
                      kind: "FragmentSpread",
                      name: "BookPage_book",
                    },
                    {
                      args: null,
                      kind: "FragmentSpread",
                      name: "VersionsTab_proposals",
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataVersionAccepted",
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
      name: "VersionsTabAcceptMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "acceptMetadataVersion",
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
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "work",
                      plural: false,
                      selections: [
                        v3 /*:: as any*/,
                        v8 /*:: as any*/,
                        v9 /*:: as any*/,
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
                                  selections: v11 /*:: as any*/,
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
                              selections: v11 /*:: as any*/,
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
                        {
                          alias: "detailAuthors",
                          args: v12 /*:: as any*/,
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
                                  selections: v13 /*:: as any*/,
                                  storageKey: null,
                                },
                                v14 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v15 /*:: as any*/,
                          ],
                          storageKey: "authors(first:20)",
                        },
                        {
                          alias: "detailAuthors",
                          args: v12 /*:: as any*/,
                          filters: null,
                          handle: "connection",
                          key: "BookPage_detailAuthors",
                          kind: "LinkedHandle",
                          name: "authors",
                        },
                        v16 /*:: as any*/,
                        v17 /*:: as any*/,
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
                        v18 /*:: as any*/,
                        v8 /*:: as any*/,
                        v9 /*:: as any*/,
                        v16 /*:: as any*/,
                        v17 /*:: as any*/,
                        v19 /*:: as any*/,
                        v20 /*:: as any*/,
                        v21 /*:: as any*/,
                        v22 /*:: as any*/,
                        v23 /*:: as any*/,
                        v3 /*:: as any*/,
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
                      concreteType: "ReadingRevision",
                      kind: "LinkedField",
                      name: "readingState",
                      plural: false,
                      selections: [
                        v18 /*:: as any*/,
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
                        v3 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    v22 /*:: as any*/,
                    v21 /*:: as any*/,
                    v23 /*:: as any*/,
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
                      args: v12 /*:: as any*/,
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
                              selections: v13 /*:: as any*/,
                              storageKey: null,
                            },
                            v14 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                        v15 /*:: as any*/,
                      ],
                      storageKey: "tags(first:20)",
                    },
                    {
                      alias: null,
                      args: v12 /*:: as any*/,
                      filters: null,
                      handle: "connection",
                      key: "BookPage_tags",
                      kind: "LinkedHandle",
                      name: "tags",
                    },
                    v19 /*:: as any*/,
                    v20 /*:: as any*/,
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
                          args: v12 /*:: as any*/,
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
                                      name: "matchType",
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
                                      concreteType: null,
                                      kind: "LinkedField",
                                      name: "newValue",
                                      plural: false,
                                      selections: [
                                        v2 /*:: as any*/,
                                        {
                                          kind: "InlineFragment",
                                          selections: [
                                            {
                                              alias: null,
                                              args: null,
                                              kind: "ScalarField",
                                              name: "text",
                                              storageKey: null,
                                            },
                                          ],
                                          type: "MetadataProposalText",
                                          abstractKey: null,
                                        },
                                        {
                                          kind: "InlineFragment",
                                          selections: [
                                            {
                                              alias: null,
                                              args: null,
                                              kind: "ScalarField",
                                              name: "integer",
                                              storageKey: null,
                                            },
                                          ],
                                          type: "MetadataProposalInteger",
                                          abstractKey: null,
                                        },
                                        {
                                          kind: "InlineFragment",
                                          selections: [
                                            {
                                              alias: null,
                                              args: null,
                                              kind: "ScalarField",
                                              name: "strings",
                                              storageKey: null,
                                            },
                                          ],
                                          type: "MetadataProposalStringList",
                                          abstractKey: null,
                                        },
                                      ],
                                      storageKey: null,
                                    },
                                    v2 /*:: as any*/,
                                  ],
                                  storageKey: null,
                                },
                                v14 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v15 /*:: as any*/,
                          ],
                          storageKey: "pending(first:20)",
                        },
                        {
                          alias: null,
                          args: v12 /*:: as any*/,
                          filters: null,
                          handle: "connection",
                          key: "VersionsTab_proposals_pending",
                          kind: "LinkedHandle",
                          name: "pending",
                        },
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "MetadataVersionAccepted",
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
      id: "6fa05c47ee955fb36eab7fb213e3fe3781e613b723287c2b683929c5095f552e",
      metadata: {},
      name: "VersionsTabAcceptMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "6ea71471c5fa0b5546954fa36524f370";

export default node;

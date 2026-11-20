/**
 * @generated SignedSource<<3b7167eb2475baf71c22f6a9a3d2fbbe>>
 * @relayHash b648724d96872021e38b9b297f601129
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID ec2759c39cacd09f31ef179693a65c622c7ae753e8f124cd05ba6ff53ec235cf

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type RejectMetadataVersionInput = {
  id: string;
  versionId: string;
};
export type mutationsRejectMetadataVersionMutation$variables = {
  input: RejectMetadataVersionInput;
};
export type mutationsRejectMetadataVersionMutation$data = {
  readonly rejectMetadataVersion:
    | {
        readonly __typename: "MetadataVersionRejected";
        readonly manifestation: {
          readonly id: string;
          readonly " $fragmentSpreads": FragmentRefs<"readerDetailBook" | "readerVersions">;
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
export type mutationsRejectMetadataVersionMutation = {
  response: mutationsRejectMetadataVersionMutation$data;
  variables: mutationsRejectMetadataVersionMutation$variables;
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
          name: "message",
          storageKey: null,
        },
      ],
      type: "NotFound",
      abstractKey: null,
    },
    v5 = [
      {
        kind: "Literal",
        name: "first",
        value: 20,
      },
    ],
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v7 = [v3 /*:: as any*/, v6 /*:: as any*/, v2 /*:: as any*/],
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v9 = {
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
      name: "mutationsRejectMetadataVersionMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "rejectMetadataVersion",
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
              type: "MetadataVersionRejected",
              abstractKey: null,
            },
            v4 /*:: as any*/,
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
      name: "mutationsRejectMetadataVersionMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "rejectMetadataVersion",
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
                          args: v5 /*:: as any*/,
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
                                  selections: v7 /*:: as any*/,
                                  storageKey: null,
                                },
                                v8 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v9 /*:: as any*/,
                          ],
                          storageKey: "authors(first:20)",
                        },
                        {
                          alias: null,
                          args: v5 /*:: as any*/,
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
                              selections: [v3 /*:: as any*/, v6 /*:: as any*/],
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
                          args: v5 /*:: as any*/,
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
                                v8 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v9 /*:: as any*/,
                          ],
                          storageKey: "pending(first:20)",
                        },
                        {
                          alias: null,
                          args: v5 /*:: as any*/,
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
                      args: v5 /*:: as any*/,
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
                              selections: v7 /*:: as any*/,
                              storageKey: null,
                            },
                            v8 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                        v9 /*:: as any*/,
                      ],
                      storageKey: "tags(first:20)",
                    },
                    {
                      alias: null,
                      args: v5 /*:: as any*/,
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
              type: "MetadataVersionRejected",
              abstractKey: null,
            },
            v4 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "ec2759c39cacd09f31ef179693a65c622c7ae753e8f124cd05ba6ff53ec235cf",
      metadata: {},
      name: "mutationsRejectMetadataVersionMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "5d9373b8426642e47a704bed2691c2bb";

export default node;

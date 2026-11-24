/**
 * @generated SignedSource<<902cd67b1b40cd7fe87b5b6a83acf533>>
 * @relayHash 76f3f9ff53058029bc9b0273b54fc166
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID c3dda713a824f60280d9d5cb3906e1aced441b5371398045dd332fac6205f6e9

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type UndoMetadataInput = {
  baseline: string;
  id: string;
  operationId: string;
};
export type useCellEditUndoMutation$variables = {
  input: UndoMetadataInput;
};
export type useCellEditUndoMutation$data = {
  readonly undoMetadata:
    | {
        readonly __typename: "EditTooLarge";
        readonly message: string;
      }
    | {
        readonly __typename: "FieldsLocked";
        readonly message: string;
      }
    | {
        readonly __typename: "MetadataRestored";
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
export type useCellEditUndoMutation = {
  response: useCellEditUndoMutation$data;
  variables: useCellEditUndoMutation$variables;
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
    v3 = [
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "message",
        storageKey: null,
      },
    ],
    v4 = {
      kind: "InlineFragment",
      selections: v3 /*:: as any*/,
      type: "StaleBaseline",
      abstractKey: null,
    },
    v5 = {
      kind: "InlineFragment",
      selections: v3 /*:: as any*/,
      type: "FieldsLocked",
      abstractKey: null,
    },
    v6 = {
      kind: "InlineFragment",
      selections: v3 /*:: as any*/,
      type: "UndoConflict",
      abstractKey: null,
    },
    v7 = {
      kind: "InlineFragment",
      selections: v3 /*:: as any*/,
      type: "UndoUnavailable",
      abstractKey: null,
    },
    v8 = {
      kind: "InlineFragment",
      selections: v3 /*:: as any*/,
      type: "EditTooLarge",
      abstractKey: null,
    },
    v9 = {
      kind: "InlineFragment",
      selections: v3 /*:: as any*/,
      type: "NotFound",
      abstractKey: null,
    },
    v10 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
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
      name: "subtitle",
      storageKey: null,
    },
    v13 = [
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "name",
        storageKey: null,
      },
      v10 /*:: as any*/,
    ],
    v14 = {
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
      name: "useCellEditUndoMutation",
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
              type: "MetadataRestored",
              abstractKey: null,
            },
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
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
      name: "useCellEditUndoMutation",
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
                {
                  alias: null,
                  args: null,
                  concreteType: "Manifestation",
                  kind: "LinkedField",
                  name: "manifestation",
                  plural: false,
                  selections: [
                    v10 /*:: as any*/,
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
                        v10 /*:: as any*/,
                        v11 /*:: as any*/,
                        v12 /*:: as any*/,
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
                                  selections: v13 /*:: as any*/,
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
                              selections: v13 /*:: as any*/,
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
                        v14 /*:: as any*/,
                        v11 /*:: as any*/,
                        v12 /*:: as any*/,
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
                        v10 /*:: as any*/,
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
                        v14 /*:: as any*/,
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
                        v10 /*:: as any*/,
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
              type: "MetadataRestored",
              abstractKey: null,
            },
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            v6 /*:: as any*/,
            v7 /*:: as any*/,
            v8 /*:: as any*/,
            v9 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "c3dda713a824f60280d9d5cb3906e1aced441b5371398045dd332fac6205f6e9",
      metadata: {},
      name: "useCellEditUndoMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "6f5fb0d0b3818a40f790846ec6eb8700";

export default node;

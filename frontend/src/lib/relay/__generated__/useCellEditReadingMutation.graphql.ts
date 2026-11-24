/**
 * @generated SignedSource<<a9e15c53e125f69a5cb74b953898252c>>
 * @relayHash 538158e8288e617b1d3f28bb67708ca6
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 154ef3549bb4842c64d6b497a0a5a4395f61e28ee6efc5a6f3ec918eaab989f7

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ReadingStatus =
  | "ABANDONED"
  | "FINISHED"
  | "ON_HOLD"
  | "READING"
  | "WANT_TO_READ"
  | "%future added value";
export type UpdateReadingStateInput = {
  baseline: string;
  id: string;
  patch: ReadingPatch;
};
export type ReadingPatch = {
  notes?: string | null | undefined;
  rating?: number | null | undefined;
  status?: ReadingStatus | null | undefined;
};
export type useCellEditReadingMutation$variables = {
  input: UpdateReadingStateInput;
};
export type useCellEditReadingMutation$data = {
  readonly updateReadingState:
    | {
        readonly __typename: "MetadataInvalid";
        readonly message: string;
      }
    | {
        readonly __typename: "NotFound";
        readonly message: string;
      }
    | {
        readonly __typename: "ReadingStateUpdated";
        readonly manifestation: {
          readonly " $fragmentSpreads": FragmentRefs<
            "BookCard_book" | "LibraryTable_cell" | "useCellEdit_batchRows"
          >;
        };
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
export type useCellEditReadingMutation = {
  response: useCellEditReadingMutation$data;
  variables: useCellEditReadingMutation$variables;
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
      type: "MetadataInvalid",
      abstractKey: null,
    },
    v6 = {
      kind: "InlineFragment",
      selections: v3 /*:: as any*/,
      type: "NotFound",
      abstractKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
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
    v10 = [
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "name",
        storageKey: null,
      },
      v7 /*:: as any*/,
    ],
    v11 = {
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
      name: "useCellEditReadingMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "updateReadingState",
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
                    {
                      args: null,
                      kind: "FragmentSpread",
                      name: "useCellEdit_batchRows",
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "ReadingStateUpdated",
              abstractKey: null,
            },
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            v6 /*:: as any*/,
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
      name: "useCellEditReadingMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "updateReadingState",
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
                    v7 /*:: as any*/,
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
                        v7 /*:: as any*/,
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
                                  selections: v10 /*:: as any*/,
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
                              selections: v10 /*:: as any*/,
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
                        v11 /*:: as any*/,
                        v8 /*:: as any*/,
                        v9 /*:: as any*/,
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
                        v7 /*:: as any*/,
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
                        v11 /*:: as any*/,
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
                        v7 /*:: as any*/,
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
              type: "ReadingStateUpdated",
              abstractKey: null,
            },
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            v6 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "154ef3549bb4842c64d6b497a0a5a4395f61e28ee6efc5a6f3ec918eaab989f7",
      metadata: {},
      name: "useCellEditReadingMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "3a8daf3d73da3ce5540efe5780afe071";

export default node;

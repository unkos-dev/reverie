/**
 * @generated SignedSource<<fc899af4a22b5f326c5841bfecad4c78>>
 * @relayHash 06bcbe912a02c4a01738a3a362e2e23a
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 5d202b4b1f6fc03a243ab2228d712f64718c9a71ca1b4ffcabb9a8031d5abb41

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ReadingStatus =
  | "ABANDONED"
  | "FINISHED"
  | "ON_HOLD"
  | "READING"
  | "WANT_TO_READ"
  | "%future added value";
export type useCellEditReloadQuery$variables = {
  id: string;
};
export type useCellEditReloadQuery$data = {
  readonly manifestation:
    | {
        readonly metadata: {
          readonly etag: string;
          readonly isbn13: string | null | undefined;
          readonly pages: number | null | undefined;
          readonly subtitle: string | null | undefined;
          readonly title: string;
        };
        readonly readingState: {
          readonly etag: string;
          readonly rating: number | null | undefined;
          readonly status: ReadingStatus | null | undefined;
        };
        readonly " $fragmentSpreads": FragmentRefs<"LibraryTable_cell">;
      }
    | null
    | undefined;
};
export type useCellEditReloadQuery = {
  response: useCellEditReloadQuery$data;
  variables: useCellEditReloadQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "id",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "pages",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "status",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "rating",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
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
      v9 /*:: as any*/,
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "useCellEditReloadQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
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
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [
                v2 /*:: as any*/,
                v3 /*:: as any*/,
                v4 /*:: as any*/,
                v5 /*:: as any*/,
                v6 /*:: as any*/,
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
              selections: [v2 /*:: as any*/, v7 /*:: as any*/, v8 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "useCellEditReloadQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
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
                v3 /*:: as any*/,
                v4 /*:: as any*/,
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
                v2 /*:: as any*/,
                v3 /*:: as any*/,
                v4 /*:: as any*/,
                v5 /*:: as any*/,
                v6 /*:: as any*/,
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
              selections: [v2 /*:: as any*/, v7 /*:: as any*/, v8 /*:: as any*/, v9 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "5d202b4b1f6fc03a243ab2228d712f64718c9a71ca1b4ffcabb9a8031d5abb41",
      metadata: {},
      name: "useCellEditReloadQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "3a8be3b9958945bf9517ecd8dc4068d7";

export default node;

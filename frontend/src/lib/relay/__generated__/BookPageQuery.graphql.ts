/**
 * @generated SignedSource<<d4908f8402df5b258ce1a0760688080c>>
 * @relayHash 63bb1260dc310dab7aa1fc290bae9abe
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 1ee1b651f18e317db5ddd55fe3b3904f826a9fddb52c6bfbcb5caacf386d12c9

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type BookPageQuery$variables = {
  id: string;
};
export type BookPageQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly " $fragmentSpreads": FragmentRefs<"BookPage_book">;
      }
    | null
    | undefined;
};
export type BookPageQuery = {
  response: BookPageQuery$data;
  variables: BookPageQuery$variables;
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
      name: "id",
      storageKey: null,
    },
    v3 = [
      {
        kind: "Literal",
        name: "first",
        value: 20,
      },
    ],
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v5 = [
      v2 /*:: as any*/,
      v4 /*:: as any*/,
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "__typename",
        storageKey: null,
      },
    ],
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v7 = {
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
      metadata: {
        throwOnFieldError: true,
      },
      name: "BookPageQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              args: null,
              kind: "FragmentSpread",
              name: "BookPage_book",
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
      name: "BookPageQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "Work",
              kind: "LinkedField",
              name: "work",
              plural: false,
              selections: [
                v2 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "title",
                  storageKey: null,
                },
                {
                  alias: "detailAuthors",
                  args: v3 /*:: as any*/,
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
                          selections: v5 /*:: as any*/,
                          storageKey: null,
                        },
                        v6 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    v7 /*:: as any*/,
                  ],
                  storageKey: "authors(first:20)",
                },
                {
                  alias: "detailAuthors",
                  args: v3 /*:: as any*/,
                  filters: null,
                  handle: "connection",
                  key: "BookPage_detailAuthors",
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
                      selections: [v2 /*:: as any*/, v4 /*:: as any*/],
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
              ],
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
              args: v3 /*:: as any*/,
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
                      selections: v5 /*:: as any*/,
                      storageKey: null,
                    },
                    v6 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                v7 /*:: as any*/,
              ],
              storageKey: "tags(first:20)",
            },
            {
              alias: null,
              args: v3 /*:: as any*/,
              filters: null,
              handle: "connection",
              key: "BookPage_tags",
              kind: "LinkedHandle",
              name: "tags",
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
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "1ee1b651f18e317db5ddd55fe3b3904f826a9fddb52c6bfbcb5caacf386d12c9",
      metadata: {},
      name: "BookPageQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "bd7de5b13e50e19437360ae3a4b567dd";

export default node;

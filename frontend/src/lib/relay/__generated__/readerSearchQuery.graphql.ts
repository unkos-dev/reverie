/**
 * @generated SignedSource<<65f451f1b257af3a79f36ac8f1ef8062>>
 * @relayHash 7181ef67d79bf0c930294303d34f5e7a
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 4c63b8ec2a4b2e5747357139fbdec606bf9a0017265ede261283594f43459f29

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSearchQuery$variables = {
  q: string;
};
export type readerSearchQuery$data = {
  readonly search: ReadonlyArray<{
    readonly manifestation:
      | {
          readonly " $fragmentSpreads": FragmentRefs<"readerBook">;
        }
      | null
      | undefined;
    readonly snippet: string | null | undefined;
  }>;
};
export type readerSearchQuery = {
  response: readerSearchQuery$data;
  variables: readerSearchQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "q",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "q",
        variableName: "q",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "snippet",
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
      name: "title",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "subtitle",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "description",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "language",
      storageKey: null,
    },
    v8 = [
      v3 /*:: as any*/,
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "name",
        storageKey: null,
      },
    ],
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "position",
      storageKey: null,
    },
    v10 = [
      {
        kind: "Literal",
        name: "first",
        value: 5,
      },
    ],
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
          name: "hasNextPage",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "hasPreviousPage",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "startCursor",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "endCursor",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    v13 = [
      {
        alias: null,
        args: null,
        concreteType: "AuthorEdge",
        kind: "LinkedField",
        name: "edges",
        plural: true,
        selections: [
          v11 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "role",
            storageKey: null,
          },
          v9 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "Author",
            kind: "LinkedField",
            name: "node",
            plural: false,
            selections: v8 /*:: as any*/,
            storageKey: null,
          },
        ],
        storageKey: null,
      },
      v12 /*:: as any*/,
    ],
    v14 = {
      alias: null,
      args: v10 /*:: as any*/,
      concreteType: "ExternalIdentifierConnection",
      kind: "LinkedField",
      name: "identifiers",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "ExternalIdentifierEdge",
          kind: "LinkedField",
          name: "edges",
          plural: true,
          selections: [
            v11 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "ExternalIdentifier",
              kind: "LinkedField",
              name: "node",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "level",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "scheme",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "value",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        v12 /*:: as any*/,
      ],
      storageKey: "identifiers(first:5)",
    },
    v15 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "isbn13",
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
      name: "pages",
      storageKey: null,
    },
    v18 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publisher",
      storageKey: null,
    },
    v19 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "publicationDate",
      storageKey: null,
    },
    v20 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "contentRating",
      storageKey: null,
    },
    v21 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "etag",
      storageKey: null,
    },
    v22 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "rating",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerSearchQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "SearchHit",
          kind: "LinkedField",
          name: "search",
          plural: true,
          selections: [
            v2 /*:: as any*/,
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
                  name: "readerBook",
                },
              ],
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
      name: "readerSearchQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "SearchHit",
          kind: "LinkedField",
          name: "search",
          plural: true,
          selections: [
            v2 /*:: as any*/,
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
                    v4 /*:: as any*/,
                    v5 /*:: as any*/,
                    v6 /*:: as any*/,
                    v7 /*:: as any*/,
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
                          selections: v8 /*:: as any*/,
                          storageKey: null,
                        },
                        v9 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: v10 /*:: as any*/,
                      concreteType: "AuthorConnection",
                      kind: "LinkedField",
                      name: "authors",
                      plural: false,
                      selections: v13 /*:: as any*/,
                      storageKey: "authors(first:5)",
                    },
                    {
                      alias: null,
                      args: v10 /*:: as any*/,
                      concreteType: "AuthorConnection",
                      kind: "LinkedField",
                      name: "contributors",
                      plural: false,
                      selections: v13 /*:: as any*/,
                      storageKey: "contributors(first:5)",
                    },
                    v14 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                v15 /*:: as any*/,
                v16 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "isbnMismatch",
                  storageKey: null,
                },
                v17 /*:: as any*/,
                v18 /*:: as any*/,
                v19 /*:: as any*/,
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
                  kind: "ScalarField",
                  name: "updatedAt",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "coverUrl",
                  storageKey: null,
                },
                v20 /*:: as any*/,
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
                  concreteType: "MetadataSnapshot",
                  kind: "LinkedField",
                  name: "metadata",
                  plural: false,
                  selections: [
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
                    v21 /*:: as any*/,
                    v4 /*:: as any*/,
                    v5 /*:: as any*/,
                    v6 /*:: as any*/,
                    v7 /*:: as any*/,
                    v16 /*:: as any*/,
                    v15 /*:: as any*/,
                    v17 /*:: as any*/,
                    v18 /*:: as any*/,
                    v19 /*:: as any*/,
                    v20 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  concreteType: "ReadingState",
                  kind: "LinkedField",
                  name: "readingState",
                  plural: false,
                  selections: [
                    v21 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "status",
                      storageKey: null,
                    },
                    v22 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "notes",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "progressPct",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "startedAt",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "finishedAt",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "lastReadAt",
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
                {
                  alias: null,
                  args: v10 /*:: as any*/,
                  concreteType: "GenreConnection",
                  kind: "LinkedField",
                  name: "genres",
                  plural: false,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      concreteType: "GenreEdge",
                      kind: "LinkedField",
                      name: "edges",
                      plural: true,
                      selections: [
                        v11 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          concreteType: "Genre",
                          kind: "LinkedField",
                          name: "node",
                          plural: false,
                          selections: v8 /*:: as any*/,
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                    v12 /*:: as any*/,
                  ],
                  storageKey: "genres(first:5)",
                },
                {
                  alias: null,
                  args: v10 /*:: as any*/,
                  concreteType: "MoodConnection",
                  kind: "LinkedField",
                  name: "moods",
                  plural: false,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      concreteType: "MoodEdge",
                      kind: "LinkedField",
                      name: "edges",
                      plural: true,
                      selections: [
                        v11 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          concreteType: "Mood",
                          kind: "LinkedField",
                          name: "node",
                          plural: false,
                          selections: v8 /*:: as any*/,
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                    v12 /*:: as any*/,
                  ],
                  storageKey: "moods(first:5)",
                },
                {
                  alias: null,
                  args: [
                    {
                      kind: "Literal",
                      name: "first",
                      value: 10,
                    },
                  ],
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
                        v11 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          concreteType: "Tag",
                          kind: "LinkedField",
                          name: "node",
                          plural: false,
                          selections: v8 /*:: as any*/,
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                    v12 /*:: as any*/,
                  ],
                  storageKey: "tags(first:10)",
                },
                v14 /*:: as any*/,
                {
                  alias: null,
                  args: v10 /*:: as any*/,
                  concreteType: "ExternalRatingConnection",
                  kind: "LinkedField",
                  name: "ratings",
                  plural: false,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      concreteType: "ExternalRatingEdge",
                      kind: "LinkedField",
                      name: "edges",
                      plural: true,
                      selections: [
                        v11 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          concreteType: "ExternalRating",
                          kind: "LinkedField",
                          name: "node",
                          plural: false,
                          selections: [
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "source",
                              storageKey: null,
                            },
                            v22 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "scale",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "reviewCount",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "fetchedAt",
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                    v12 /*:: as any*/,
                  ],
                  storageKey: "ratings(first:5)",
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
      id: "4c63b8ec2a4b2e5747357139fbdec606bf9a0017265ede261283594f43459f29",
      metadata: {},
      name: "readerSearchQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "e8522fc5acc528c5ac8c39f22d98fda8";

export default node;

/**
 * @generated SignedSource<<c7e0916380c47bd6fbcc2bdce8094b49>>
 * @relayHash ddd9085f6d19b63f1db56d62aec1ba99
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID ca248d7c667682fc9af3e9d9cf03c8209575cd55ad5fde9a2702b01ac61f149f

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ReadingStatusFilter =
  | "ABANDONED"
  | "FINISHED"
  | "ON_HOLD"
  | "READING"
  | "UNREAD"
  | "WANT_TO_READ"
  | "%future added value";
export type BooksFilter = {
  author?: ReadonlyArray<string>;
  authorAny?: ReadonlyArray<string>;
  authorNone?: ReadonlyArray<string>;
  coverEmpty?: boolean | null | undefined;
  createdAtGte?: string | null | undefined;
  createdAtLt?: string | null | undefined;
  enrichmentFailed?: boolean | null | undefined;
  genre?: ReadonlyArray<string>;
  genreAny?: ReadonlyArray<string>;
  genreNone?: ReadonlyArray<string>;
  isbn13Contains?: string | null | undefined;
  isbn13Empty?: boolean | null | undefined;
  isbn13Eq?: string | null | undefined;
  mood?: ReadonlyArray<string>;
  moodAny?: ReadonlyArray<string>;
  moodNone?: ReadonlyArray<string>;
  needsReview?: boolean | null | undefined;
  pagesEmpty?: boolean | null | undefined;
  pagesGte?: number | null | undefined;
  pagesLte?: number | null | undefined;
  q?: string | null | undefined;
  ratingEmpty?: boolean | null | undefined;
  ratingGte?: number | null | undefined;
  ratingLte?: number | null | undefined;
  series?: string | null | undefined;
  seriesAny?: ReadonlyArray<string>;
  shelf?: string | null | undefined;
  shelfAny?: ReadonlyArray<string>;
  statusAny?: ReadonlyArray<ReadingStatusFilter>;
  statusNone?: ReadonlyArray<ReadingStatusFilter>;
  subtitleContains?: string | null | undefined;
  subtitleEmpty?: boolean | null | undefined;
  tag?: ReadonlyArray<string>;
  tagAny?: ReadonlyArray<string>;
  tagNone?: ReadonlyArray<string>;
  titleContains?: string | null | undefined;
  titleEq?: string | null | undefined;
  titleNe?: string | null | undefined;
};
export type LibraryPageCountsRefreshQuery$variables = {
  filter?: BooksFilter | null | undefined;
  shelfIds: ReadonlyArray<string>;
};
export type LibraryPageCountsRefreshQuery$data = {
  readonly libraryFacets: {
    readonly " $fragmentSpreads": FragmentRefs<"FilterRail_facets" | "LibraryToolbar_facets">;
  };
  readonly librarySummary: {
    readonly " $fragmentSpreads": FragmentRefs<"LibraryToolbar_summary" | "ShelfHeader_summary">;
  };
};
export type LibraryPageCountsRefreshQuery = {
  response: LibraryPageCountsRefreshQuery$data;
  variables: LibraryPageCountsRefreshQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "filter",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "shelfIds",
    },
    v2 = [
      {
        kind: "Variable",
        name: "shelfIds",
        variableName: "shelfIds",
      },
    ],
    v3 = [
      {
        kind: "Variable",
        name: "filter",
        variableName: "filter",
      },
    ],
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "finished",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "reading",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "unread",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/],
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "LibraryPageCountsRefreshQuery",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: "LibrarySummary",
          kind: "LinkedField",
          name: "librarySummary",
          plural: false,
          selections: [
            {
              args: null,
              kind: "FragmentSpread",
              name: "ShelfHeader_summary",
            },
            {
              args: null,
              kind: "FragmentSpread",
              name: "LibraryToolbar_summary",
            },
          ],
          storageKey: null,
        },
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "LibraryFacets",
          kind: "LinkedField",
          name: "libraryFacets",
          plural: false,
          selections: [
            {
              args: null,
              kind: "FragmentSpread",
              name: "FilterRail_facets",
            },
            {
              args: null,
              kind: "FragmentSpread",
              name: "LibraryToolbar_facets",
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
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "LibraryPageCountsRefreshQuery",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: "LibrarySummary",
          kind: "LinkedField",
          name: "librarySummary",
          plural: false,
          selections: [
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "totalCount",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              concreteType: "ReadingStateCounts",
              kind: "LinkedField",
              name: "readingStates",
              plural: false,
              selections: [v4 /*:: as any*/, v5 /*:: as any*/, v6 /*:: as any*/],
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              concreteType: "NeedsAttentionCounts",
              kind: "LinkedField",
              name: "needsAttention",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "toReview",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "enrichmentFailed",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "noIsbn",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "noCover",
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
          args: v3 /*:: as any*/,
          concreteType: "LibraryFacets",
          kind: "LinkedField",
          name: "libraryFacets",
          plural: false,
          selections: [
            {
              alias: null,
              args: null,
              concreteType: "ReadingStateCounts",
              kind: "LinkedField",
              name: "readingStates",
              plural: false,
              selections: [
                v6 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "wantToRead",
                  storageKey: null,
                },
                v5 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "onHold",
                  storageKey: null,
                },
                v4 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "abandoned",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              concreteType: "ShelfFacetCount",
              kind: "LinkedField",
              name: "shelves",
              plural: true,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "shelfId",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "count",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "matchingCount",
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "ca248d7c667682fc9af3e9d9cf03c8209575cd55ad5fde9a2702b01ac61f149f",
      metadata: {},
      name: "LibraryPageCountsRefreshQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "eabb5bf1766be539c6e04ba5d8a81eef";

export default node;

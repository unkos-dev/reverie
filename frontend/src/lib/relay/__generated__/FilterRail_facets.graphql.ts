/**
 * @generated SignedSource<<f4129e151afa39ddb40500fe595d542a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type FilterRail_facets$data = {
  readonly readingStates: {
    readonly " $fragmentSpreads": FragmentRefs<"editors_statusCounts">;
  };
  readonly shelves: ReadonlyArray<{
    readonly " $fragmentSpreads": FragmentRefs<"FilterRail_shelfCounts">;
  }>;
  readonly " $fragmentType": "FilterRail_facets";
};
export type FilterRail_facets$key = {
  readonly " $data"?: FilterRail_facets$data;
  readonly " $fragmentSpreads": FragmentRefs<"FilterRail_facets">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "FilterRail_facets",
  selections: [
    {
      alias: null,
      args: null,
      concreteType: "ReadingStateCounts",
      kind: "LinkedField",
      name: "readingStates",
      plural: false,
      selections: [
        {
          args: null,
          kind: "FragmentSpread",
          name: "editors_statusCounts",
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
          args: null,
          kind: "FragmentSpread",
          name: "FilterRail_shelfCounts",
        },
      ],
      storageKey: null,
    },
  ],
  type: "LibraryFacets",
  abstractKey: null,
};

(node as any).hash = "9309f34c8250dc6ad7584f48e9f8b911";

export default node;

/**
 * @generated SignedSource<<a50453a2a34a70deffaf67ffa3da28b3>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type SeriesPage_work$data = {
  readonly manifestations: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly coverUrl: string;
        readonly id: string;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
    };
  };
  readonly title: string;
  readonly " $fragmentSpreads": FragmentRefs<"SeriesPage_authors">;
  readonly " $fragmentType": "SeriesPage_work";
};
export type SeriesPage_work$key = {
  readonly " $data"?: SeriesPage_work$data;
  readonly " $fragmentSpreads": FragmentRefs<"SeriesPage_work">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "SeriesPage_work",
  selections: [
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    {
      args: null,
      kind: "FragmentSpread",
      name: "SeriesPage_authors",
    },
    {
      alias: null,
      args: [
        {
          kind: "Literal",
          name: "first",
          value: 1,
        },
      ],
      concreteType: "ManifestationConnection",
      kind: "LinkedField",
      name: "manifestations",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "ManifestationEdge",
          kind: "LinkedField",
          name: "edges",
          plural: true,
          selections: [
            {
              alias: null,
              args: null,
              concreteType: "Manifestation",
              kind: "LinkedField",
              name: "node",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "id",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "coverUrl",
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
              name: "endCursor",
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
      storageKey: "manifestations(first:1)",
    },
  ],
  type: "Work",
  abstractKey: null,
};

(node as any).hash = "2089b1ad65dd9c0174a9a23acf98302a";

export default node;

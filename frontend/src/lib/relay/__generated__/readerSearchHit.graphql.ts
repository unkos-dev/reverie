/**
 * @generated SignedSource<<b5fc14db9619b45f60b82d341c2b1b10>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSearchHit$data = {
  readonly id: string;
  readonly work:
    | {
        readonly authors: {
          readonly edges: ReadonlyArray<{
            readonly node: {
              readonly name: string;
            };
          }>;
          readonly pageInfo: {
            readonly endCursor: string | null | undefined;
            readonly hasNextPage: boolean;
          };
        };
        readonly title: string;
      }
    | null
    | undefined;
  readonly " $fragmentType": "readerSearchHit";
};
export type readerSearchHit$key = {
  readonly " $data"?: readerSearchHit$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerSearchHit">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: null,
  name: "readerSearchHit",
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
      concreteType: "Work",
      kind: "LinkedField",
      name: "work",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "title",
          storageKey: null,
        },
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
                  selections: [
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "name",
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
          storageKey: "authors(first:5)",
        },
      ],
      storageKey: null,
    },
  ],
  type: "Manifestation",
  abstractKey: null,
};

(node as any).hash = "8fdf5a039d3ce0986733a59cddaeba1b";

export default node;

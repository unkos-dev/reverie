/**
 * @generated SignedSource<<e5083b6026b64d9b8df989605b7fa8c7>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type CommandPalette_hit$data = {
  readonly manifestation:
    | {
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
      }
    | null
    | undefined;
  readonly snippet:
    | ReadonlyArray<{
        readonly highlighted: boolean;
        readonly text: string;
      }>
    | null
    | undefined;
  readonly " $fragmentType": "CommandPalette_hit";
};
export type CommandPalette_hit$key = {
  readonly " $data"?: CommandPalette_hit$data;
  readonly " $fragmentSpreads": FragmentRefs<"CommandPalette_hit">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "CommandPalette_hit",
  selections: [
    {
      alias: null,
      args: null,
      concreteType: "SearchSnippetPart",
      kind: "LinkedField",
      name: "snippet",
      plural: true,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "text",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "highlighted",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      concreteType: "Manifestation",
      kind: "LinkedField",
      name: "manifestation",
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
      storageKey: null,
    },
  ],
  type: "SearchHit",
  abstractKey: null,
};

(node as any).hash = "97a669b9b9661609880b006c817d0533";

export default node;

/**
 * @generated SignedSource<<6bd6d2cc761159dce3c0d39fe57b1864>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type VersionsTab_proposal$data = {
  readonly confidenceScore: number;
  readonly id: string;
  readonly matchType: string;
  readonly newValue:
    | {
        readonly __typename: "MetadataProposalInteger";
        readonly integer: number;
      }
    | {
        readonly __typename: "MetadataProposalStringList";
        readonly strings: ReadonlyArray<string>;
      }
    | {
        readonly __typename: "MetadataProposalText";
        readonly text: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
  readonly source: string;
  readonly " $fragmentType": "VersionsTab_proposal";
};
export type VersionsTab_proposal$key = {
  readonly " $data"?: VersionsTab_proposal$data;
  readonly " $fragmentSpreads": FragmentRefs<"VersionsTab_proposal">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: {
    throwOnFieldError: true,
  },
  name: "VersionsTab_proposal",
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
      name: "source",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "matchType",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "confidenceScore",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      concreteType: null,
      kind: "LinkedField",
      name: "newValue",
      plural: false,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "__typename",
          storageKey: null,
        },
        {
          kind: "InlineFragment",
          selections: [
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "text",
              storageKey: null,
            },
          ],
          type: "MetadataProposalText",
          abstractKey: null,
        },
        {
          kind: "InlineFragment",
          selections: [
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "integer",
              storageKey: null,
            },
          ],
          type: "MetadataProposalInteger",
          abstractKey: null,
        },
        {
          kind: "InlineFragment",
          selections: [
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "strings",
              storageKey: null,
            },
          ],
          type: "MetadataProposalStringList",
          abstractKey: null,
        },
      ],
      storageKey: null,
    },
  ],
  type: "MetadataVersion",
  abstractKey: null,
};

(node as any).hash = "37573e391fbf931fc0c5c6c19ef199c7";

export default node;

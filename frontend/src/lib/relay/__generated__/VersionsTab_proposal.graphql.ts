/**
 * @generated SignedSource<<b465ea26fa9614f0067ad96f81b78ad2>>
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
        readonly __typename: "MetadataProposalContributors";
        readonly contributors: ReadonlyArray<{
          readonly code: string | null | undefined;
          readonly name: string;
        }>;
      }
    | {
        readonly __typename: "MetadataProposalInteger";
        readonly integer: number;
      }
    | {
        readonly __typename: "MetadataProposalSeries";
        readonly name: string;
        readonly position: number | null | undefined;
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

const node: ReaderFragment = (function () {
  var v0 = {
    alias: null,
    args: null,
    kind: "ScalarField",
    name: "name",
    storageKey: null,
  };
  return {
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
          {
            kind: "InlineFragment",
            selections: [
              v0 /*:: as any*/,
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "position",
                storageKey: null,
              },
            ],
            type: "MetadataProposalSeries",
            abstractKey: null,
          },
          {
            kind: "InlineFragment",
            selections: [
              {
                alias: null,
                args: null,
                concreteType: "MetadataProposalContributor",
                kind: "LinkedField",
                name: "contributors",
                plural: true,
                selections: [
                  v0 /*:: as any*/,
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "code",
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
            ],
            type: "MetadataProposalContributors",
            abstractKey: null,
          },
        ],
        storageKey: null,
      },
    ],
    type: "MetadataVersion",
    abstractKey: null,
  };
})();

(node as any).hash = "bf5c0ed4d7c05afe1aaf50c8fe25ee40";

export default node;

/**
 * @generated SignedSource<<5b402221208b2c50985bf16323a73cce>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerWork$data = {
  readonly authors: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly id: string;
        readonly name: string;
      };
      readonly position: number;
      readonly role: string;
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
  readonly contributors: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly id: string;
        readonly name: string;
      };
      readonly position: number;
      readonly role: string;
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
  readonly description: string | null | undefined;
  readonly id: string;
  readonly identifiers: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly level: string;
        readonly scheme: string;
        readonly value: string;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
  readonly language: string | null | undefined;
  readonly selectedSeries:
    | {
        readonly node: {
          readonly id: string;
          readonly name: string;
        };
        readonly position: number | null | undefined;
      }
    | null
    | undefined;
  readonly subtitle: string | null | undefined;
  readonly title: string;
  readonly " $fragmentType": "readerWork";
};
export type readerWork$key = {
  readonly " $data"?: readerWork$data;
  readonly " $fragmentSpreads": FragmentRefs<"readerWork">;
};

const node: ReaderFragment = (function () {
  var v0 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v1 = [
      v0 /*:: as any*/,
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "name",
        storageKey: null,
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "position",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v4 = {
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
    v5 = [
      {
        alias: null,
        args: null,
        concreteType: "AuthorEdge",
        kind: "LinkedField",
        name: "edges",
        plural: true,
        selections: [
          v3 /*:: as any*/,
          {
            alias: null,
            args: null,
            kind: "ScalarField",
            name: "role",
            storageKey: null,
          },
          v2 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "Author",
            kind: "LinkedField",
            name: "node",
            plural: false,
            selections: v1 /*:: as any*/,
            storageKey: null,
          },
        ],
        storageKey: null,
      },
      v4 /*:: as any*/,
    ];
  return {
    argumentDefinitions: [],
    kind: "Fragment",
    metadata: null,
    name: "readerWork",
    selections: [
      v0 /*:: as any*/,
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "title",
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "subtitle",
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
            selections: v1 /*:: as any*/,
            storageKey: null,
          },
          v2 /*:: as any*/,
        ],
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "AuthorConnection",
        kind: "LinkedField",
        name: "authors",
        plural: false,
        selections: v5 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: null,
        concreteType: "AuthorConnection",
        kind: "LinkedField",
        name: "contributors",
        plural: false,
        selections: v5 /*:: as any*/,
        storageKey: null,
      },
      {
        alias: null,
        args: null,
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
              v3 /*:: as any*/,
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
          v4 /*:: as any*/,
        ],
        storageKey: null,
      },
    ],
    type: "Work",
    abstractKey: null,
  };
})();

(node as any).hash = "a2397278b76248ffc3bd2cd25f127e1f";

export default node;

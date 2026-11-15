/**
 * @generated SignedSource<<a4f51f75726c1e7b3b839e33d111e2df>>
 * @relayHash 802b8268121dcc677e5df8779269eafd
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 2c837b92eb377451d1c2f5dc777282b27df5b6a798d4076b61e19bf19cc7c59b

import { ConcreteRequest } from "relay-runtime";
export type readerShelvesQuery$variables = {
  after?: string | null | undefined;
  count?: number | null | undefined;
};
export type readerShelvesQuery$data = {
  readonly shelves: {
    readonly edges: ReadonlyArray<{
      readonly cursor: string;
      readonly node: {
        readonly createdAt: string;
        readonly id: string;
        readonly isSystem: boolean;
        readonly itemCount: number;
        readonly name: string;
        readonly updatedAt: string;
      };
    }>;
    readonly pageInfo: {
      readonly endCursor: string | null | undefined;
      readonly hasNextPage: boolean;
      readonly hasPreviousPage: boolean;
      readonly startCursor: string | null | undefined;
    };
  };
};
export type readerShelvesQuery = {
  response: readerShelvesQuery$data;
  variables: readerShelvesQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "after",
    },
    v1 = {
      defaultValue: 50,
      kind: "LocalArgument",
      name: "count",
    },
    v2 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "after",
            variableName: "after",
          },
          {
            kind: "Variable",
            name: "first",
            variableName: "count",
          },
        ],
        concreteType: "ShelfConnection",
        kind: "LinkedField",
        name: "shelves",
        plural: false,
        selections: [
          {
            alias: null,
            args: null,
            concreteType: "ShelfEdge",
            kind: "LinkedField",
            name: "edges",
            plural: true,
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "cursor",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                concreteType: "Shelf",
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
                    name: "name",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "isSystem",
                    storageKey: null,
                  },
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "itemCount",
                    storageKey: null,
                  },
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
        ],
        storageKey: null,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "readerShelvesQuery",
      selections: v2 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "readerShelvesQuery",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "2c837b92eb377451d1c2f5dc777282b27df5b6a798d4076b61e19bf19cc7c59b",
      metadata: {},
      name: "readerShelvesQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "7fcebf78bbba05613333104f842f2fae";

export default node;

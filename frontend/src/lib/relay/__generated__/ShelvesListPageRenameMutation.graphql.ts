/**
 * @generated SignedSource<<d0aaeff0784aa052b851d392f3e37f09>>
 * @relayHash 15e1ecc21f74c135e38638f073d7b2ac
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 9c911f5fdf93a8e16011885789dfdbf36c1645072542702c451ab9ed85bc5f63

import { ConcreteRequest } from "relay-runtime";
export type RenameShelfInput = {
  id: string;
  name: string;
};
export type ShelvesListPageRenameMutation$variables = {
  input: RenameShelfInput;
};
export type ShelvesListPageRenameMutation$data = {
  readonly renameShelf:
    | {
        readonly __typename: "NotFound";
        readonly message: string;
      }
    | {
        readonly __typename: "ShelfNameInvalid";
        readonly message: string;
      }
    | {
        readonly __typename: "ShelfNameTaken";
        readonly message: string;
      }
    | {
        readonly __typename: "ShelfRenamed";
        readonly shelf: {
          readonly id: string;
          readonly name: string;
          readonly updatedAt: string;
        };
      }
    | {
        readonly __typename: "SystemShelfImmutable";
        readonly message: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type ShelvesListPageRenameMutation = {
  response: ShelvesListPageRenameMutation$data;
  variables: ShelvesListPageRenameMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "input",
      },
    ],
    v1 = [
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "message",
        storageKey: null,
      },
    ],
    v2 = [
      {
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "input",
            variableName: "input",
          },
        ],
        concreteType: null,
        kind: "LinkedField",
        name: "renameShelf",
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
                concreteType: "Shelf",
                kind: "LinkedField",
                name: "shelf",
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
                    name: "updatedAt",
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
            ],
            type: "ShelfRenamed",
            abstractKey: null,
          },
          {
            kind: "InlineFragment",
            selections: v1 /*:: as any*/,
            type: "ShelfNameInvalid",
            abstractKey: null,
          },
          {
            kind: "InlineFragment",
            selections: v1 /*:: as any*/,
            type: "ShelfNameTaken",
            abstractKey: null,
          },
          {
            kind: "InlineFragment",
            selections: v1 /*:: as any*/,
            type: "SystemShelfImmutable",
            abstractKey: null,
          },
          {
            kind: "InlineFragment",
            selections: v1 /*:: as any*/,
            type: "NotFound",
            abstractKey: null,
          },
        ],
        storageKey: null,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "ShelvesListPageRenameMutation",
      selections: v2 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "ShelvesListPageRenameMutation",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "9c911f5fdf93a8e16011885789dfdbf36c1645072542702c451ab9ed85bc5f63",
      metadata: {},
      name: "ShelvesListPageRenameMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "2e0db742de152bab8556fdace5fe3b79";

export default node;

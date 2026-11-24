/**
 * @generated SignedSource<<d909453b9e55a47d3b7ecb21922c6b35>>
 * @relayHash b5886aa8571384b08d2ba2519242bb59
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID c112e68841610f2b459733492153adfee10df806703d3948c66c76e286be0ac4

import { ConcreteRequest } from "relay-runtime";
export type AddShelfItemInput = {
  manifestationId: string;
  shelfId: string;
};
export type LibraryPageAddShelfItemMutation$variables = {
  input: AddShelfItemInput;
};
export type LibraryPageAddShelfItemMutation$data = {
  readonly addShelfItem:
    | {
        readonly __typename: "NotFound";
        readonly message: string;
      }
    | {
        readonly __typename: "ShelfItemAdded";
        readonly __typename: "ShelfItemAdded";
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type LibraryPageAddShelfItemMutation = {
  response: LibraryPageAddShelfItemMutation$data;
  variables: LibraryPageAddShelfItemMutation$variables;
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
        args: [
          {
            kind: "Variable",
            name: "input",
            variableName: "input",
          },
        ],
        concreteType: null,
        kind: "LinkedField",
        name: "addShelfItem",
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
                name: "message",
                storageKey: null,
              },
            ],
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
      name: "LibraryPageAddShelfItemMutation",
      selections: v1 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "LibraryPageAddShelfItemMutation",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "c112e68841610f2b459733492153adfee10df806703d3948c66c76e286be0ac4",
      metadata: {},
      name: "LibraryPageAddShelfItemMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "1b1eef6faba34c5c63262864bda87492";

export default node;

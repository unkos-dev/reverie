/**
 * @generated SignedSource<<af50e1f95cb6a4bbdd4959de28a08729>>
 * @relayHash 8b9c5656311efb3e62c543ba1e46ee54
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 60fae5847eed4d077c2037f5d1d3e7f0c4d6182e4e51f62ef593928bd4119c74

import { ConcreteRequest } from "relay-runtime";
export type RenameShelfInput = {
  id: string;
  name: string;
};
export type mutationsRenameShelfMutation$variables = {
  input: RenameShelfInput;
};
export type mutationsRenameShelfMutation$data = {
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
export type mutationsRenameShelfMutation = {
  response: mutationsRenameShelfMutation$data;
  variables: mutationsRenameShelfMutation$variables;
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
      name: "mutationsRenameShelfMutation",
      selections: v2 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsRenameShelfMutation",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "60fae5847eed4d077c2037f5d1d3e7f0c4d6182e4e51f62ef593928bd4119c74",
      metadata: {},
      name: "mutationsRenameShelfMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "be9137a01b4f0da8d068b9a2d3284323";

export default node;

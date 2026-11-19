/**
 * @generated SignedSource<<c048fb8f3f07bd56d8d5a2ba9decb9c2>>
 * @relayHash b6334b3e6ada6308708095bcbc3396a2
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 6388cbbbb92a88ca8a228f1ab5e0c20164cf4189a1c7dc158c2f1d2a08a08fea

import { ConcreteRequest } from "relay-runtime";
export type CreateShelfInput = {
  name: string;
};
export type mutationsCreateShelfMutation$variables = {
  connections: ReadonlyArray<string>;
  input: CreateShelfInput;
};
export type mutationsCreateShelfMutation$data = {
  readonly createShelf:
    | {
        readonly __typename: "ShelfCreated";
        readonly edge: {
          readonly cursor: string;
          readonly node: {
            readonly createdAt: string;
            readonly id: string;
            readonly isSystem: boolean;
            readonly itemCount: number;
            readonly name: string;
            readonly updatedAt: string;
          };
        };
      }
    | {
        readonly __typename: "ShelfNameInvalid";
        readonly message: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type mutationsCreateShelfMutation = {
  response: mutationsCreateShelfMutation$data;
  variables: mutationsCreateShelfMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "connections",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "input",
    },
    v2 = [
      {
        kind: "Variable",
        name: "input",
        variableName: "input",
      },
    ],
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      concreteType: "ShelfEdge",
      kind: "LinkedField",
      name: "edge",
      plural: false,
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
    v5 = {
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
      type: "ShelfNameInvalid",
      abstractKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsCreateShelfMutation",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "createShelf",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [v4 /*:: as any*/],
              type: "ShelfCreated",
              abstractKey: null,
            },
            v5 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "mutationsCreateShelfMutation",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "createShelf",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                v4 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  filters: null,
                  handle: "appendEdge",
                  key: "",
                  kind: "LinkedHandle",
                  name: "edge",
                  handleArgs: [
                    {
                      kind: "Variable",
                      name: "connections",
                      variableName: "connections",
                    },
                  ],
                },
              ],
              type: "ShelfCreated",
              abstractKey: null,
            },
            v5 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "6388cbbbb92a88ca8a228f1ab5e0c20164cf4189a1c7dc158c2f1d2a08a08fea",
      metadata: {},
      name: "mutationsCreateShelfMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "10c67a59ce117e076b79d0e014213d97";

export default node;

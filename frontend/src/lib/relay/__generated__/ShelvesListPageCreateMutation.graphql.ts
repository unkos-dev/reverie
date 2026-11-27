/**
 * @generated SignedSource<<0bfef9f775d1ca74878d34c5eb98e0a6>>
 * @relayHash aaa4596b817d8368a09c07eac99b9536
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 976e93eb04ff57e75134b07bd5d451422cfa3770b34b2ee845fb895ae3f8e3ee

import { ConcreteRequest } from "relay-runtime";
export type CreateShelfInput = {
  name: string;
};
export type ShelvesListPageCreateMutation$variables = {
  connections: ReadonlyArray<string>;
  input: CreateShelfInput;
};
export type ShelvesListPageCreateMutation$data = {
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
        readonly __typename: "ShelfNameTaken";
        readonly message: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type ShelvesListPageCreateMutation = {
  response: ShelvesListPageCreateMutation$data;
  variables: ShelvesListPageCreateMutation$variables;
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
    v5 = [
      {
        alias: null,
        args: null,
        kind: "ScalarField",
        name: "message",
        storageKey: null,
      },
    ],
    v6 = {
      kind: "InlineFragment",
      selections: v5 /*:: as any*/,
      type: "ShelfNameInvalid",
      abstractKey: null,
    },
    v7 = {
      kind: "InlineFragment",
      selections: v5 /*:: as any*/,
      type: "ShelfNameTaken",
      abstractKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "ShelvesListPageCreateMutation",
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
            v6 /*:: as any*/,
            v7 /*:: as any*/,
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
      name: "ShelvesListPageCreateMutation",
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
            v6 /*:: as any*/,
            v7 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "976e93eb04ff57e75134b07bd5d451422cfa3770b34b2ee845fb895ae3f8e3ee",
      metadata: {},
      name: "ShelvesListPageCreateMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "6dfd0a92eabc285f3f492ed8c63f1f6b";

export default node;

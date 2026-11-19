/**
 * @generated SignedSource<<21be94de489dc1dd7103879f6866b146>>
 * @relayHash 430545e5d43659c7ff6523fe9e453abb
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 6f58e94c487ad8eca7f813febbbb6ff9ee9303daaa63afd3eb29bc6c43d04394

import { ConcreteRequest } from "relay-runtime";
export type DeleteShelfInput = {
  id: string;
};
export type mutationsDeleteShelfMutation$variables = {
  connections: ReadonlyArray<string>;
  input: DeleteShelfInput;
};
export type mutationsDeleteShelfMutation$data = {
  readonly deleteShelf:
    | {
        readonly __typename: "NotFound";
        readonly message: string;
      }
    | {
        readonly __typename: "ShelfDeleted";
        readonly deletedShelfId: string;
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
export type mutationsDeleteShelfMutation = {
  response: mutationsDeleteShelfMutation$data;
  variables: mutationsDeleteShelfMutation$variables;
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
      kind: "ScalarField",
      name: "deletedShelfId",
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
      type: "SystemShelfImmutable",
      abstractKey: null,
    },
    v7 = {
      kind: "InlineFragment",
      selections: v5 /*:: as any*/,
      type: "NotFound",
      abstractKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsDeleteShelfMutation",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "deleteShelf",
          plural: false,
          selections: [
            v3 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [v4 /*:: as any*/],
              type: "ShelfDeleted",
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
      name: "mutationsDeleteShelfMutation",
      selections: [
        {
          alias: null,
          args: v2 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "deleteShelf",
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
                  handle: "deleteEdge",
                  key: "",
                  kind: "ScalarHandle",
                  name: "deletedShelfId",
                  handleArgs: [
                    {
                      kind: "Variable",
                      name: "connections",
                      variableName: "connections",
                    },
                  ],
                },
              ],
              type: "ShelfDeleted",
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
      id: "6f58e94c487ad8eca7f813febbbb6ff9ee9303daaa63afd3eb29bc6c43d04394",
      metadata: {},
      name: "mutationsDeleteShelfMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "0ba2614bd509d3a902e4935fc7bdcea9";

export default node;

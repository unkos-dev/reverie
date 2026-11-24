/**
 * @generated SignedSource<<1752a22ae3fe00f5429a95fb1b7143d7>>
 * @relayHash d502dceb15ab13f898d26fd072719677
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID a0e264ca4eb2cd71953169015faee81a95a600e5c3d9ea01c0b49ca639a2bc23

import { ConcreteRequest } from "relay-runtime";
export type DeleteShelfInput = {
  id: string;
};
export type ShelvesListPageDeleteMutation$variables = {
  connections: ReadonlyArray<string>;
  input: DeleteShelfInput;
};
export type ShelvesListPageDeleteMutation$data = {
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
export type ShelvesListPageDeleteMutation = {
  response: ShelvesListPageDeleteMutation$data;
  variables: ShelvesListPageDeleteMutation$variables;
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
      name: "ShelvesListPageDeleteMutation",
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
      name: "ShelvesListPageDeleteMutation",
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
      id: "a0e264ca4eb2cd71953169015faee81a95a600e5c3d9ea01c0b49ca639a2bc23",
      metadata: {},
      name: "ShelvesListPageDeleteMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "ac6c3fca21e983a8db223099b6bd35d4";

export default node;

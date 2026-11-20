/**
 * @generated SignedSource<<7b7902cca62d9b60d5adedb226aa9932>>
 * @relayHash 8222559dbfa9824672908b69f74e7ba3
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 10db4782bf106b40245e92bdfa5b0c1eec3ac123a8b56cdc7f00f1a9df10ca5f

import { ConcreteRequest } from "relay-runtime";
export type FieldLockScope = "MANIFESTATION" | "WORK" | "%future added value";
export type UnlockMetadataFieldInput = {
  entityType: FieldLockScope;
  fieldName: string;
  id: string;
};
export type mutationsUnlockMetadataFieldMutation$variables = {
  input: UnlockMetadataFieldInput;
};
export type mutationsUnlockMetadataFieldMutation$data = {
  readonly unlockMetadataField:
    | {
        readonly __typename: "MetadataFieldLockRemoved";
        readonly manifestation: {
          readonly id: string;
        };
      }
    | {
        readonly __typename: "NotFound";
        readonly message: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type mutationsUnlockMetadataFieldMutation = {
  response: mutationsUnlockMetadataFieldMutation$data;
  variables: mutationsUnlockMetadataFieldMutation$variables;
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
        name: "unlockMetadataField",
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
                concreteType: "Manifestation",
                kind: "LinkedField",
                name: "manifestation",
                plural: false,
                selections: [
                  {
                    alias: null,
                    args: null,
                    kind: "ScalarField",
                    name: "id",
                    storageKey: null,
                  },
                ],
                storageKey: null,
              },
            ],
            type: "MetadataFieldLockRemoved",
            abstractKey: null,
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
      name: "mutationsUnlockMetadataFieldMutation",
      selections: v1 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsUnlockMetadataFieldMutation",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "10db4782bf106b40245e92bdfa5b0c1eec3ac123a8b56cdc7f00f1a9df10ca5f",
      metadata: {},
      name: "mutationsUnlockMetadataFieldMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "2b58fb6cc990f7dc7738b6bcb806004b";

export default node;

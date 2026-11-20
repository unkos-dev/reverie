/**
 * @generated SignedSource<<51739c4372a2d277c01a5e5feaad797b>>
 * @relayHash 2a6a49b157d94d5be7794f4b933df886
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID e5b59e6564acc8388fd23ac9a94bd612332147a641fff37c135d1d9695c22ff8

import { ConcreteRequest } from "relay-runtime";
export type FieldLockScope = "MANIFESTATION" | "WORK" | "%future added value";
export type LockMetadataFieldInput = {
  entityType: FieldLockScope;
  fieldName: string;
  id: string;
};
export type mutationsLockMetadataFieldMutation$variables = {
  input: LockMetadataFieldInput;
};
export type mutationsLockMetadataFieldMutation$data = {
  readonly lockMetadataField:
    | {
        readonly __typename: "MetadataFieldLockAdded";
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
export type mutationsLockMetadataFieldMutation = {
  response: mutationsLockMetadataFieldMutation$data;
  variables: mutationsLockMetadataFieldMutation$variables;
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
        name: "lockMetadataField",
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
            type: "MetadataFieldLockAdded",
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
      name: "mutationsLockMetadataFieldMutation",
      selections: v1 /*:: as any*/,
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsLockMetadataFieldMutation",
      selections: v1 /*:: as any*/,
    },
    params: {
      id: "e5b59e6564acc8388fd23ac9a94bd612332147a641fff37c135d1d9695c22ff8",
      metadata: {},
      name: "mutationsLockMetadataFieldMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "6d3f322f8434af147bb43d274cfa2e0e";

export default node;

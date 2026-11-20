/**
 * @generated SignedSource<<7cd1465a3b3428ad2164d078a48a51b0>>
 * @relayHash eb5d011bef07bebc847de9a99022bd1f
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID e27db75e86521a41a304dea91984d0ed4f7028434a5a7ef4089a12abc1d4a105

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type mutationsResolveFormQuery$variables = {
  id: string;
  operationId?: string | null | undefined;
  requestId?: string | null | undefined;
};
export type mutationsResolveFormQuery$data = {
  readonly metadataOperation: {
    readonly metadata: {
      readonly " $fragmentSpreads": FragmentRefs<"mutationsFormSnapshot">;
    };
    readonly outcome: string;
  };
};
export type mutationsResolveFormQuery = {
  response: mutationsResolveFormQuery$data;
  variables: mutationsResolveFormQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "operationId",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "requestId",
    },
    v3 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
      {
        kind: "Variable",
        name: "operationId",
        variableName: "operationId",
      },
      {
        kind: "Variable",
        name: "requestId",
        variableName: "requestId",
      },
    ],
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "outcome",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/],
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "mutationsResolveFormQuery",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "MetadataOperation",
          kind: "LinkedField",
          name: "metadataOperation",
          plural: false,
          selections: [
            v4 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
              plural: false,
              selections: [
                {
                  args: null,
                  kind: "FragmentSpread",
                  name: "mutationsFormSnapshot",
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v0 /*:: as any*/, v2 /*:: as any*/, v1 /*:: as any*/],
      kind: "Operation",
      name: "mutationsResolveFormQuery",
      selections: [
        {
          alias: null,
          args: v3 /*:: as any*/,
          concreteType: "MetadataOperation",
          kind: "LinkedField",
          name: "metadataOperation",
          plural: false,
          selections: [
            v4 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "MetadataRevision",
              kind: "LinkedField",
              name: "metadata",
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
                  name: "workId",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "manifestationId",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "etag",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "title",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "subtitle",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "description",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "language",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "publisher",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "publicationDate",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "isbn10",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "isbn13",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "isbnMismatch",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "pages",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "contentRating",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "e27db75e86521a41a304dea91984d0ed4f7028434a5a7ef4089a12abc1d4a105",
      metadata: {},
      name: "mutationsResolveFormQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "4d4adee88d7350d4efbaa821e8a1d4d8";

export default node;

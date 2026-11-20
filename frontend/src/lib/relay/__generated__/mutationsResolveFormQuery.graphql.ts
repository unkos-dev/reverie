/**
 * @generated SignedSource<<0c9ebfe9b332281c1c0221f4935a3720>>
 * @relayHash fb406e0149d4adb4c7e6ee00b6190675
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 4f942cd9cbe67f8ac8ca55b8a395013620b22d0f349610ea1f668370161bf2f5

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type OperationOutcome = "RESTORED" | "SAVED" | "UNKNOWN" | "%future added value";
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
    readonly outcome: OperationOutcome;
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
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "4f942cd9cbe67f8ac8ca55b8a395013620b22d0f349610ea1f668370161bf2f5",
      metadata: {},
      name: "mutationsResolveFormQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "4e97f2a13c8e671a13db1f252453e220";

export default node;

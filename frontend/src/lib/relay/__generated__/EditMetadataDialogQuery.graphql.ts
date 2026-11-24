/**
 * @generated SignedSource<<6ea33a8bc444012fcc256860be740a27>>
 * @relayHash 084ea0c4042594cc1d23a7f72ded4466
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 81f74b3fe42ed2c97804eec390ff5edc5dada86816714504aeecc0e6f2ccfbb4

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type EditMetadataDialogQuery$variables = {
  id: string;
};
export type EditMetadataDialogQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly metadata: {
          readonly " $fragmentSpreads": FragmentRefs<"EditMetadataDialog_opening">;
        };
      }
    | null
    | undefined;
};
export type EditMetadataDialogQuery = {
  response: EditMetadataDialogQuery$data;
  variables: EditMetadataDialogQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "id",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "EditMetadataDialogQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
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
                  name: "EditMetadataDialog_opening",
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
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "EditMetadataDialogQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Manifestation",
          kind: "LinkedField",
          name: "manifestation",
          plural: false,
          selections: [
            v2 /*:: as any*/,
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
                v2 /*:: as any*/,
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "81f74b3fe42ed2c97804eec390ff5edc5dada86816714504aeecc0e6f2ccfbb4",
      metadata: {},
      name: "EditMetadataDialogQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "cb75a909e2ba85016e3ebfacb6ac51ba";

export default node;

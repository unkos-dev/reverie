/**
 * @generated SignedSource<<86e844465c25b22fc0b0e1cf368af18a>>
 * @relayHash da60b713a9502ef870b3b02087c3cad8
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID ae5f84dce56d3f691a7693fb6b75127c01e3a371914e9d664453b7fb8d149c63

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerLibraryPreferencesQuery$variables = Record<PropertyKey, never>;
export type readerLibraryPreferencesQuery$data = {
  readonly libraryPreferences: {
    readonly " $fragmentSpreads": FragmentRefs<"readerLibraryPreferences">;
  };
};
export type readerLibraryPreferencesQuery = {
  response: readerLibraryPreferencesQuery$data;
  variables: readerLibraryPreferencesQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "sortStack",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: [],
      kind: "Fragment",
      metadata: null,
      name: "readerLibraryPreferencesQuery",
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "LibraryPreferences",
          kind: "LinkedField",
          name: "libraryPreferences",
          plural: false,
          selections: [
            {
              args: null,
              kind: "FragmentSpread",
              name: "readerLibraryPreferences",
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
      argumentDefinitions: [],
      kind: "Operation",
      name: "readerLibraryPreferencesQuery",
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "LibraryPreferences",
          kind: "LinkedField",
          name: "libraryPreferences",
          plural: false,
          selections: [
            v0 /*:: as any*/,
            v1 /*:: as any*/,
            v2 /*:: as any*/,
            v3 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "LibraryPreferenceDefaults",
              kind: "LinkedField",
              name: "defaults",
              plural: false,
              selections: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/, v3 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "ae5f84dce56d3f691a7693fb6b75127c01e3a371914e9d664453b7fb8d149c63",
      metadata: {},
      name: "readerLibraryPreferencesQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "40e786b71402de332d175c0a003801ad";

export default node;

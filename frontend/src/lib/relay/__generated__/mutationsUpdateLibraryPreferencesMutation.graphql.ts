/**
 * @generated SignedSource<<4ff9c8107d270169ec5d7770ad8e61f8>>
 * @relayHash 7a73f16c64ee06d9316f1cd54387c85d
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 35b15ea91d8ba43e91003fb447e36595e3e1f2b8df07eac070e0403567311dc6

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type LibraryDensityInput = "COMFORTABLE" | "COMPACT" | "%future added value";
export type LibraryViewInput = "GRID" | "TABLE" | "%future added value";
export type LibraryPreferencesPatch = {
  density?: LibraryDensityInput | null | undefined;
  hiddenColumns?: ReadonlyArray<string> | null | undefined;
  sortStack?: string | null | undefined;
  view?: LibraryViewInput | null | undefined;
};
export type mutationsUpdateLibraryPreferencesMutation$variables = {
  patch: LibraryPreferencesPatch;
};
export type mutationsUpdateLibraryPreferencesMutation$data = {
  readonly updateLibraryPreferences: {
    readonly " $fragmentSpreads": FragmentRefs<"readerLibraryPreferences">;
  };
};
export type mutationsUpdateLibraryPreferencesMutation = {
  response: mutationsUpdateLibraryPreferencesMutation$data;
  variables: mutationsUpdateLibraryPreferencesMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "patch",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "patch",
        variableName: "patch",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "sortStack",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsUpdateLibraryPreferencesMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "LibraryPreferences",
          kind: "LinkedField",
          name: "updateLibraryPreferences",
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
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsUpdateLibraryPreferencesMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "LibraryPreferences",
          kind: "LinkedField",
          name: "updateLibraryPreferences",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            v3 /*:: as any*/,
            v4 /*:: as any*/,
            v5 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "LibraryPreferenceDefaults",
              kind: "LinkedField",
              name: "defaults",
              plural: false,
              selections: [v2 /*:: as any*/, v3 /*:: as any*/, v4 /*:: as any*/, v5 /*:: as any*/],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "35b15ea91d8ba43e91003fb447e36595e3e1f2b8df07eac070e0403567311dc6",
      metadata: {},
      name: "mutationsUpdateLibraryPreferencesMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "a1292adc56c31b09586132353766ff2f";

export default node;

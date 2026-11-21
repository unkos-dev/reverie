/**
 * @generated SignedSource<<7caf29f017020aefce14dae3da85aeb1>>
 * @relayHash 48ddf899a215c846530eb64bcb9e4462
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 614548b269274356bd63a5e27863f37ca6e735f01810f53dc6e1fb802a7b57cb

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type LibraryPageLabelsRefetchQuery$variables = {
  authorIds: ReadonlyArray<string>;
  seriesIds: ReadonlyArray<string>;
  shelfIds: ReadonlyArray<string>;
};
export type LibraryPageLabelsRefetchQuery$data = {
  readonly " $fragmentSpreads": FragmentRefs<"LibraryPage_labels">;
};
export type LibraryPageLabelsRefetchQuery = {
  response: LibraryPageLabelsRefetchQuery$data;
  variables: LibraryPageLabelsRefetchQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "authorIds",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "seriesIds",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "shelfIds",
      },
    ],
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v3 = [v1 /*:: as any*/, v2 /*:: as any*/];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "LibraryPageLabelsRefetchQuery",
      selections: [
        {
          args: [
            {
              kind: "Variable",
              name: "authorIds",
              variableName: "authorIds",
            },
            {
              kind: "Variable",
              name: "seriesIds",
              variableName: "seriesIds",
            },
            {
              kind: "Variable",
              name: "shelfIds",
              variableName: "shelfIds",
            },
          ],
          kind: "FragmentSpread",
          name: "LibraryPage_labels",
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "LibraryPageLabelsRefetchQuery",
      selections: [
        {
          alias: null,
          args: [
            {
              kind: "Variable",
              name: "ids",
              variableName: "authorIds",
            },
          ],
          concreteType: "Author",
          kind: "LinkedField",
          name: "authorLabels",
          plural: true,
          selections: v3 /*:: as any*/,
          storageKey: null,
        },
        {
          alias: null,
          args: [
            {
              kind: "Variable",
              name: "ids",
              variableName: "seriesIds",
            },
          ],
          concreteType: "Series",
          kind: "LinkedField",
          name: "seriesLabels",
          plural: true,
          selections: v3 /*:: as any*/,
          storageKey: null,
        },
        {
          alias: null,
          args: [
            {
              kind: "Variable",
              name: "ids",
              variableName: "shelfIds",
            },
          ],
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelfLabels",
          plural: true,
          selections: [
            v1 /*:: as any*/,
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "itemCount",
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "614548b269274356bd63a5e27863f37ca6e735f01810f53dc6e1fb802a7b57cb",
      metadata: {},
      name: "LibraryPageLabelsRefetchQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "f4af5e99251eeb172a1ac47db9166dd7";

export default node;

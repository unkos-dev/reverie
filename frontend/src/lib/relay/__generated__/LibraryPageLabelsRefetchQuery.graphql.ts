/**
 * @generated SignedSource<<d9825c8a2c49eff9ac684acdd5a30c17>>
 * @relayHash 3952bdc5db9dc00f17d9858c4bc6d25f
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 18a4881ae5bf1df4033db8a923c4c4f26dc7aa1ada2435a4a0c93b183acb9be9

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
      id: "18a4881ae5bf1df4033db8a923c4c4f26dc7aa1ada2435a4a0c93b183acb9be9",
      metadata: {},
      name: "LibraryPageLabelsRefetchQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "cf6f505f1d0180638dcd88f6410fce6e";

export default node;

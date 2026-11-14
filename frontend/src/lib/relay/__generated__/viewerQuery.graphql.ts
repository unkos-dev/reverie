/**
 * @generated SignedSource<<e869a556fcaecb286365f4a5a7c93140>>
 * @relayHash dca8664be16b3790a294a03036855fef
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 27e70c1a4678ff8126a20d1a363664ea46b33f6f459c3d589fb91da9170ebe35

import { ConcreteRequest } from "relay-runtime";
export type viewerQuery$variables = Record<PropertyKey, never>;
export type viewerQuery$data = {
  readonly viewer: {
    readonly displayName: string;
    readonly id: string;
    readonly isChild: boolean;
  };
};
export type viewerQuery = {
  response: viewerQuery$data;
  variables: viewerQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
    {
      alias: null,
      args: null,
      concreteType: "Viewer",
      kind: "LinkedField",
      name: "viewer",
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
          name: "displayName",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "isChild",
          storageKey: null,
        },
      ],
      storageKey: null,
    },
  ];
  return {
    fragment: {
      argumentDefinitions: [],
      kind: "Fragment",
      metadata: null,
      name: "viewerQuery",
      selections: v0 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [],
      kind: "Operation",
      name: "viewerQuery",
      selections: v0 /*:: as any*/,
    },
    params: {
      id: "27e70c1a4678ff8126a20d1a363664ea46b33f6f459c3d589fb91da9170ebe35",
      metadata: {},
      name: "viewerQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "095d48616e589457677a522a53233c32";

export default node;

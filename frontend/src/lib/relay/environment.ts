import { Environment, Network, ROOT_ID, RecordSource, Store } from "relay-runtime";
import { loadQuery, type PreloadedQuery } from "react-relay";
import type { GraphQLTaggedNode, MissingFieldHandler, OperationType } from "relay-runtime";

import { graphqlNetwork } from "@/api/graphql";
import { authMeQueryOptions } from "@/hooks/useAuthMe";
import { queryClient } from "@/lib/query/client";
import { clearLibraryVisits } from "./library-visit";

const LABEL_FIELDS = new Set(["authorLabels", "seriesLabels", "shelfLabels"]);

const missingFieldHandlers: MissingFieldHandler[] = [
  {
    kind: "pluralLinked",
    handle: (field, record, args) => {
      const ids: unknown = args.ids;
      if (record?.getDataID() !== ROOT_ID || !LABEL_FIELDS.has(field.name)) return undefined;
      return Array.isArray(ids) && ids.every((id) => typeof id === "string") ? ids : undefined;
    },
  },
];

let current: {
  account: string;
  environment: Environment;
  controller: AbortController;
  source: RecordSource;
} | null = null;

export function setRelayAccount(account: string | null): void {
  if (current?.account === account) return;
  if (current !== null) {
    current.controller.abort();
    clearLibraryVisits();
    current.source.clear();
    current.environment.getStore().notify();
    current = null;
  }
  if (account !== null) {
    const controller = new AbortController();
    const source = new RecordSource();
    current = {
      source,
      account,
      controller,
      environment: new Environment({
        network: Network.create((request, variables) =>
          graphqlNetwork(request, variables, controller.signal),
        ),
        store: new Store(source, { queryCacheExpirationTime: 5 * 60 * 1000 }),
        missingFieldHandlers,
      }),
    };
  }
}

export function relayEnvironment(): Environment {
  if (current === null) throw new Error("Relay requires a confirmed account.");
  return current.environment;
}

export function confirmedRelayEnvironment(): Environment | null {
  return current?.environment ?? null;
}

export async function preloadRouteQuery<T extends OperationType>(
  document: GraphQLTaggedNode,
  variables: T["variables"],
  signal: AbortSignal,
): Promise<PreloadedQuery<T> | null> {
  const account = await queryClient.query(authMeQueryOptions);
  const environment = current?.account === account?.id ? current?.environment : undefined;
  if (environment === undefined || signal.aborted) return null;
  const reference = loadQuery<T>(environment, document, variables, {
    fetchPolicy: "store-or-network",
  });
  signal.addEventListener(
    "abort",
    () => {
      reference.dispose();
    },
    { once: true },
  );
  return reference;
}

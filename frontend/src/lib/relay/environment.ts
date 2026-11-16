import { Environment, Network, Observable, RecordSource, Store } from "relay-runtime";
import { fetchQuery, loadQuery, type PreloadedQuery } from "react-relay";
import type { GraphQLTaggedNode, OperationType } from "relay-runtime";

import { graphqlNetwork } from "@/api/graphql";
import { authMeQueryOptions } from "@/hooks/useAuthMe";
import { queryClient } from "@/lib/query/client";
import { clearLibraryVisits } from "./library-visit";

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
        store: new Store(source),
      }),
    };
  }
}

export function relayEnvironment(): Environment {
  if (current === null) throw new Error("Relay requires a confirmed account.");
  return current.environment;
}

export type PreparedReader<T extends OperationType> = {
  reference: PreloadedQuery<T>;
  data: T["response"] | undefined;
  dispose: () => void;
};

/** Prepare one route read after account confirmation, with abort and route disposal ownership. */
export async function prepareReader<T extends OperationType>(
  document: GraphQLTaggedNode,
  variables: T["variables"],
  signal: AbortSignal,
): Promise<PreparedReader<T> | null> {
  const account = await queryClient.query(authMeQueryOptions);
  if (account === null || signal.aborted) return null;
  const environment = relayEnvironment();
  const data = await Observable.create<T["response"]>((sink) => {
    if (signal.aborted) {
      sink.complete();
      return;
    }
    const cancel = () => {
      sink.complete();
    };
    signal.addEventListener("abort", cancel, { once: true });
    const subscription = fetchQuery<T>(environment, document, variables).subscribe(sink);
    return () => {
      signal.removeEventListener("abort", cancel);
      subscription.unsubscribe();
    };
  }).toPromise();
  signal.throwIfAborted();
  if (current?.environment !== environment) {
    return null;
  }
  const reference = loadQuery<T>(environment, document, variables, { fetchPolicy: "store-only" });
  const dispose = () => {
    signal.removeEventListener("abort", dispose);
    reference.dispose();
  };
  signal.addEventListener("abort", dispose, { once: true });
  return { reference, data, dispose };
}

import { Environment, Network, RecordSource, Store } from "relay-runtime";

import { graphqlNetwork } from "@/api/graphql";

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

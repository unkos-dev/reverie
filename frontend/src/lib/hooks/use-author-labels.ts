import { useEffect, useState } from "react";
import { fetchQuery, useRelayEnvironment } from "react-relay";
import { encodeGraphQLId, decodeGraphQLId } from "@/api/graphql";
import { readerAuthorLabelsQuery } from "@/lib/relay/reader";
import type { readerAuthorLabelsQuery as LabelsQuery } from "@/lib/relay/__generated__/readerAuthorLabelsQuery.graphql";

export function useAuthorLabels(ids: readonly string[]): { labelFor: (id: string) => string } {
  const environment = useRelayEnvironment();
  const token = [...new Set(ids)].sort().slice(0, 60).join(",");
  const [result, setResult] = useState<{
    token: string;
    data?: LabelsQuery["response"];
    error?: Error;
  }>({ token: "" });
  useEffect(() => {
    if (token === "") return;
    const subscription = fetchQuery<LabelsQuery>(
      environment,
      readerAuthorLabelsQuery,
      { ids: token.split(",").map((id) => encodeGraphQLId("Author", id)) },
      { fetchPolicy: "store-or-network" },
    ).subscribe({
      next: (data) => {
        setResult({ token, data });
      },
      error: (error: Error) => {
        console.error("[useAuthorLabels] label lookup failed", error);
        setResult({ token, error });
      },
    });
    return () => {
      subscription.unsubscribe();
    };
  }, [environment, token]);
  const data = result.token === token ? result.data : undefined;
  return {
    labelFor: (id) =>
      data?.authorLabels.find(
        (row) => row.entityId != null && decodeGraphQLId("Author", row.entityId) === id,
      )?.value ?? `Author ${id.slice(0, 8)}…`,
  };
}

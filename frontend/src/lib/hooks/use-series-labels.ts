import { useEffect, useState } from "react";
import { fetchQuery, useRelayEnvironment } from "react-relay";
import { encodeGraphQLId, decodeGraphQLId } from "@/api/graphql";
import { readerSeriesLabelsQuery } from "@/lib/relay/reader";
import type { readerSeriesLabelsQuery as LabelsQuery } from "@/lib/relay/__generated__/readerSeriesLabelsQuery.graphql";

export function useSeriesLabels(ids: readonly string[]): { labelFor: (id: string) => string } {
  const environment = useRelayEnvironment();
  const token = [...new Set(ids)]
    .sort((left, right) => left.localeCompare(right))
    .slice(0, 60)
    .join(",");
  const [result, setResult] = useState<{
    token: string;
    data?: LabelsQuery["response"];
    error?: Error;
  }>({ token: "" });
  useEffect(() => {
    if (token === "") return;
    const subscription = fetchQuery<LabelsQuery>(
      environment,
      readerSeriesLabelsQuery,
      { ids: token.split(",").map((id) => encodeGraphQLId("Series", id)) },
      { fetchPolicy: "store-or-network" },
    ).subscribe({
      next: (data) => {
        setResult({ token, data });
      },
      error: (error: Error) => {
        console.error("[useSeriesLabels] label lookup failed", error);
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
      data?.seriesLabels.find(
        (row) => row.entityId != null && decodeGraphQLId("Series", row.entityId) === id,
      )?.value ?? `Series ${id.slice(0, 8)}…`,
  };
}

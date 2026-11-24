/* oxlint-disable react/only-export-components */
import { fetchQuery, graphql } from "react-relay";
import { createOperationDescriptor, getRequest, type Environment } from "relay-runtime";
import type { LoaderFunctionArgs } from "react-router";

import { authMeQueryOptions } from "@/hooks/useAuthMe";
import { restoreVisit } from "@/lib/hooks/use-library-filters";
import { queryClient } from "@/lib/query/client";
import { confirmedRelayEnvironment } from "@/lib/relay/environment";
import {
  libraryDestination,
  libraryLookups,
  libraryRequest,
  validateRestoredVisit,
  type LibraryLookups,
  type LibraryPreparation,
  type LibrarySource,
  type LibraryVisitOwner,
} from "@/lib/relay/library-visit";
import { serializeSortParam } from "@/api/books";
import { sortLevelParams } from "@/pages/library/use-library-preferences";
import type {
  libraryBootstrapQuery as BootstrapQuery,
  libraryBootstrapQuery$data,
} from "@/lib/relay/__generated__/libraryBootstrapQuery.graphql";
import type { LibraryVisit } from "@/routes/library-params";

const libraryBootstrapQuery = graphql`
  query libraryBootstrapQuery($authorIds: [ID!]!, $seriesIds: [ID!]!, $shelfIds: [ID!]!)
  @throwOnFieldError {
    viewer {
      id
      libraryPreferences {
        ...useLibraryPreferences_preferences
        sort {
          column
          direction
        }
        defaults {
          sort {
            column
            direction
          }
        }
      }
    }
    authorLabels(ids: $authorIds) {
      id
    }
    seriesLabels(ids: $seriesIds) {
      id
    }
    shelfLabels(ids: $shelfIds) {
      id
    }
  }
`;

export type PreparedLibrary = {
  owner: LibraryVisitOwner;
  preparation: LibraryPreparation;
  visit: LibraryVisit;
  initial: LibrarySource;
};

function bootstrap(
  environment: Environment,
  variables: LibraryLookups,
  signal: AbortSignal,
): Promise<libraryBootstrapQuery$data> {
  return new Promise((resolve, reject) => {
    let last: libraryBootstrapQuery$data | undefined;
    const subscription = fetchQuery<BootstrapQuery>(environment, libraryBootstrapQuery, variables, {
      fetchPolicy: "store-or-network",
    }).subscribe({
      next: (data) => {
        last = data;
      },
      complete: () => {
        signal.removeEventListener("abort", abort);
        if (last === undefined) reject(new Error("The Library bootstrap returned no data."));
        else resolve(last);
      },
      error: (error: Error) => {
        signal.removeEventListener("abort", abort);
        reject(error);
      },
    });
    function abort(): void {
      subscription.unsubscribe();
      const reason: unknown = signal.reason;
      reject(reason instanceof Error ? reason : new Error("The Library load was aborted."));
    }
    if (signal.aborted) abort();
    else signal.addEventListener("abort", abort, { once: true });
  });
}

/**
 * Read the account's preferences and check the destination's restored selections before the
 * screen query loads, so the first books request carries the stored sort and no selection the
 * viewer can no longer see.
 */
export async function loader({ request }: LoaderFunctionArgs): Promise<PreparedLibrary | null> {
  const account = await queryClient.query(authMeQueryOptions);
  const environment = confirmedRelayEnvironment();
  if (account === null || environment === null || request.signal.aborted) return null;
  const owner = libraryDestination(environment, request.url, restoreVisit);
  const restored = owner.intent.current();
  const preparation = owner.begin();
  preparation.watch(request.signal, () => {
    owner.discard(preparation);
  });
  const lookups = libraryLookups(restored.filters);
  preparation.hold(
    environment.retain(createOperationDescriptor(getRequest(libraryBootstrapQuery), lookups)),
  );
  let data: libraryBootstrapQuery$data;
  try {
    data = await bootstrap(environment, lookups, request.signal);
    request.signal.throwIfAborted();
  } catch (error) {
    owner.discard(preparation);
    throw error;
  }
  if (confirmedRelayEnvironment() !== environment) {
    owner.discard(preparation);
    return null;
  }
  const visit = validateRestoredVisit(restored, lookups, {
    authors: new Set(data.authorLabels.map((author) => author.id)),
    series: new Set(data.seriesLabels.map((series) => series.id)),
    shelves: new Set(data.shelfLabels.map((shelf) => shelf.id)),
  });
  owner.intent.restore(visit);
  if (visit !== restored) owner.intent.update(() => visit);
  const preferences = data.viewer.libraryPreferences;
  const override = sortLevelParams(preferences.sort ?? []);
  const effective =
    preferences.sort === null ? sortLevelParams(preferences.defaults.sort) : override;
  const initial = preparation.hold(
    owner.prepare(libraryRequest(visit.filters, serializeSortParam(override)), effective),
  );
  owner.settle(preparation);
  return { owner, preparation, visit, initial };
}

export { LibraryPage as Component } from "@/pages/library/LibraryPage";

/**
 * In-memory record of the confirmed account in this browser realm.
 *
 * `rememberActiveUser` establishes the account-scoped Relay environment when
 * `/auth/me` confirms an identity. `forgetActiveUser` tears it down on
 * sign-out and on the 401 path. `activeUserEpoch` advances on every account
 * change or loss so a delayed `/auth/me` response cannot recreate an
 * environment for an account that has already been forgotten.
 */

import { setRelayAccount } from "@/lib/relay/environment";

let identityEpoch = 0;
let confirmedAccount: string | null = null;

export function activeUserEpoch(): number {
  return identityEpoch;
}

/** Record the confirmed account id and select its Relay environment. */
export function rememberActiveUser(id: string, expectedEpoch = identityEpoch): void {
  if (expectedEpoch !== identityEpoch) return;
  if (confirmedAccount !== id) {
    identityEpoch += 1;
    confirmedAccount = id;
  }
  setRelayAccount(id);
}

/** Forget the confirmed account and tear down its Relay environment. */
export function forgetActiveUser(): void {
  identityEpoch += 1;
  confirmedAccount = null;
  setRelayAccount(null);
}

import type { ReactElement } from "react";

import type { PasswordLengthPolicy } from "@/api/auth.schemas";
import { Button } from "@/components/ui/button";
import { FieldDescription } from "@/components/ui/field";

type Props = {
  policy: PasswordLengthPolicy | undefined;
  failed: boolean;
  onRetry?: () => void;
  retrying?: boolean;
};

export function PasswordPolicyHint({
  policy,
  failed,
  onRetry,
  retrying = false,
}: Readonly<Props>): ReactElement {
  const message =
    policy !== undefined
      ? `Use ${String(policy.password_min_length)} to ${String(policy.password_max_length)} characters. Avoid common words or passwords from known data breaches.`
      : failed || retrying
        ? onRetry
          ? "Could not load the password policy."
          : "Could not load the password policy. Reload to try again."
        : "Loading password policy…";
  return (
    <>
      <FieldDescription role={policy === undefined && failed ? "alert" : undefined}>
        {message}
      </FieldDescription>
      {onRetry && (failed || retrying) ? (
        <Button type="button" variant="outline" size="sm" disabled={retrying} onClick={onRetry}>
          Retry password policy
        </Button>
      ) : null}
    </>
  );
}

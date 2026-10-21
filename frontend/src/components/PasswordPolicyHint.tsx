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
  let message = "Loading password policy…";
  if (policy !== undefined) {
    message = `Use ${String(policy.password_min_length)} to ${String(policy.password_max_length)} characters. Avoid common words or passwords from known data breaches.`;
  } else if (failed || retrying) {
    message = onRetry
      ? "Could not load the password policy."
      : "Could not load the password policy. Reload to try again.";
  }
  return (
    <>
      <FieldDescription role={policy === undefined && failed ? "alert" : undefined}>
        {message}
      </FieldDescription>
      {onRetry && (failed || retrying) ? (
        <Button type="button" variant="outline" size="sm" isDisabled={retrying} onPress={onRetry}>
          Retry password policy
        </Button>
      ) : null}
    </>
  );
}

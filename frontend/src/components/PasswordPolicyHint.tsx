import type { ReactElement } from "react";

import type { PasswordLengthPolicy } from "@/api/auth.schemas";
import { FieldDescription } from "@/components/ui/field";

type Props = {
  policy: PasswordLengthPolicy | undefined;
  failed: boolean;
};

export function PasswordPolicyHint({ policy, failed }: Props): ReactElement {
  const message =
    policy !== undefined
      ? `Use ${String(policy.password_min_length)} to ${String(policy.password_max_length)} characters. Avoid common words or passwords from known data breaches.`
      : failed
        ? "Could not load the password policy. Reload to try again."
        : "Loading password policy…";
  return (
    <FieldDescription role={policy === undefined && failed ? "alert" : undefined}>
      {message}
    </FieldDescription>
  );
}

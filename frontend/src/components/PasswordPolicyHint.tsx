import type { ReactElement } from "react";

import type { PasswordLengthPolicy } from "@/api/auth.schemas";
import { FieldDescription } from "@/components/ui/field";

type Props = {
  policy: PasswordLengthPolicy | undefined;
  failed: boolean;
};

export function PasswordPolicyHint({ policy, failed }: Props): ReactElement {
  let message = "Loading password policy…";
  if (policy !== undefined) {
    message = `Use ${String(policy.password_min_length)} to ${String(policy.password_max_length)} characters. Avoid common words or passwords from known data breaches.`;
  } else if (failed) {
    message = "Could not load the password policy. Reload to try again.";
  }
  return (
    <FieldDescription role={policy === undefined && failed ? "alert" : undefined}>
      {message}
    </FieldDescription>
  );
}

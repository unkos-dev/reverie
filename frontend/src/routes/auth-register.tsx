/**
 * `/register`: self-service account registration.
 *
 * NOTE: intentionally not routed in `main.tsx`. Registration today creates an
 * immediately-active account with no admin approval, so account creation stays
 * admin-provisioned and this screen is dormant. It is kept here (and tested) as
 * the starting point for an approval-gated request-access flow. Wire the route
 * up only once registration is moderated.
 *
 * Config-gated on the server (`self_registration_enabled`); a 404 means the
 * instance has it turned off, surfaced inline. A self-registered account is
 * always an adult. Registration does not establish a session (parity with
 * setup and recovery), so on success it routes to the login screen.
 *
 * Forms are uncontrolled (`FormData`).
 */
import { useMutation } from "@tanstack/react-query";
import { useActionState, type ReactElement } from "react";
import { Form, type FormProps } from "react-aria-components";
import { Link, useNavigate } from "react-router";
import { z } from "zod";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { register } from "@/api/auth";
import { RegisterSchema, validateDisplayName, validateEmail } from "@/api/auth.schemas";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Button } from "@/components/ui/button";
import { FieldError, TextInputField } from "@/components/ui/field";
import { usePasswordPolicy } from "@/hooks/usePasswordPolicy";

import { AuthShell } from "./auth-shell";

/** Route component for `/register`. */
export function Component(): ReactElement {
  const { policy, ready, isError: policyError, validate } = usePasswordPolicy();
  const navigate = useNavigate();

  const registerMutation = useMutation({
    mutationFn: ({
      email,
      displayName,
      password,
    }: {
      email: string;
      displayName: string;
      password: string;
    }) => register(email, displayName, password),
    onSuccess: () => {
      toast.success("Account created. Sign in to continue.");
      void navigate("/login");
    },
  });

  const [state, action, pending] = useActionState<
    {
      values?: { display_name: string; email: string; password: string };
      errors?: FormProps["validationErrors"];
      error?: string;
    },
    FormData
  >(async (_previous, data) => {
    const values = z
      .object({ display_name: z.string(), email: z.string(), password: z.string() })
      .parse(Object.fromEntries(data));
    if (!ready) return { values };
    const passwordError = validate(values.password);
    if (passwordError !== undefined) return { values, errors: { password: passwordError } };
    const parsed = RegisterSchema.safeParse(values);
    if (!parsed.success) return { values, errors: z.flattenError(parsed.error).fieldErrors };
    try {
      await registerMutation.mutateAsync({
        email: parsed.data.email,
        displayName: parsed.data.display_name,
        password: parsed.data.password,
      });
      return {};
    } catch (err) {
      const message =
        err instanceof ApiError && err.detail !== "" ? err.detail : "Could not create the account.";
      toast.error(message);
      return { values, error: message };
    }
  }, {});

  return (
    <AuthShell
      title="Create your account"
      description="Register to start using this instance."
      footer={
        <Link to="/login" className="hover:text-fg underline underline-offset-4">
          Already have an account? Sign in
        </Link>
      }
    >
      <Form action={action} validationErrors={state.errors} className="flex flex-col gap-4">
        <TextInputField
          name="display_name"
          defaultValue={state.values?.display_name}
          type="text"
          autoComplete="name"
          isRequired
          validate={validateDisplayName}
          label="Display name"
        />
        <TextInputField
          name="email"
          defaultValue={state.values?.email}
          type="email"
          autoComplete="email"
          isRequired
          validate={validateEmail}
          label="Email"
        />
        <TextInputField
          name="password"
          defaultValue={state.values?.password}
          type="password"
          autoComplete="new-password"
          isRequired
          validate={(value) => (ready ? (validate(value) ?? null) : null)}
          label="Password"
        >
          <PasswordPolicyHint policy={policy} failed={policyError} />
        </TextInputField>
        {!pending && state.error !== undefined ? <FieldError>{state.error}</FieldError> : null}
        <Button type="submit" isPending={pending} isDisabled={!ready}>
          Create account
        </Button>
      </Form>
    </AuthShell>
  );
}

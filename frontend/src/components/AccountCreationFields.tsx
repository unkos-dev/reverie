import type { ReactElement, ReactNode } from "react";
import type { FormProps } from "react-aria-components";
import type { z } from "zod";

import {
  type AccountCreationValuesSchema,
  validateDisplayName,
  validateEmail,
} from "@/api/auth.schemas";
import { TextInputField } from "@/components/ui/field";

export type AccountCreationState = {
  values?: z.infer<typeof AccountCreationValuesSchema>;
  errors?: FormProps["validationErrors"];
  error?: string;
};

export function AccountCreationFields({
  values,
  validatePassword,
  children,
}: Readonly<{
  values: AccountCreationState["values"];
  validatePassword: (value: string) => string | null;
  children: ReactNode;
}>): ReactElement {
  return (
    <>
      <TextInputField
        name="display_name"
        defaultValue={values?.display_name}
        type="text"
        autoComplete="name"
        isRequired
        validate={validateDisplayName}
        label="Display name"
      />
      <TextInputField
        name="email"
        defaultValue={values?.email}
        type="email"
        autoComplete="email"
        isRequired
        validate={validateEmail}
        label="Email"
      />
      <TextInputField
        name="password"
        defaultValue={values?.password}
        type="password"
        autoComplete="new-password"
        isRequired
        validate={validatePassword}
        label="Password"
      >
        {children}
      </TextInputField>
    </>
  );
}

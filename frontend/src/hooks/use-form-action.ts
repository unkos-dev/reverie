import { useActionState } from "react";
import { z } from "zod";

type FormActionState = {
  values: Partial<Record<string, string>>;
  error: string | null;
};

export function useFormAction(
  submit: (data: FormData) => Promise<string | null>,
): [FormActionState, (data: FormData) => void, boolean] {
  return useActionState<FormActionState, FormData>(
    async (_previous, data) => {
      const values = z.record(z.string(), z.string()).parse(Object.fromEntries(data));
      return { values, error: await submit(data) };
    },
    { values: {}, error: null },
  );
}

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Compose Tailwind class strings with conflict resolution. `clsx` flattens
 * the arguments (objects, arrays, falsy values, conditional strings);
 * `twMerge` then collapses conflicting utilities so a later class wins over
 * an earlier one (`cn("p-2", "p-4")` → `"p-4"` rather than
 * `"p-2 p-4"`). Required for variant-prop patterns where a caller passes a
 * class that overrides a default supplied by the component author.
 *
 * @param inputs - Any value [clsx](https://github.com/lukeed/clsx) accepts.
 * @returns Space-separated class string ready to drop into `className`.
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

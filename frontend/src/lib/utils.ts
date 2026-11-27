import { twMerge, type ClassNameValue } from "tailwind-merge";

/**
 * Compose Tailwind class strings so later conflicting utilities override earlier ones.
 */
export function cn(...inputs: ClassNameValue[]): string {
  return twMerge(...inputs);
}

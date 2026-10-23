import type { SettingsKey } from "@/api/settings";

export function labelId(key: SettingsKey): string {
  return `label-${key}`;
}

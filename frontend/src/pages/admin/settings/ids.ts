import type { SettingsKey } from "@/api/settings";

export function controlId(key: SettingsKey): string {
  return `control-${key}`;
}

export function labelId(key: SettingsKey): string {
  return `label-${key}`;
}

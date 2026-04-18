import type { SettingSection } from "../types/chat";

export const settingsSections: SettingSection[] = [
  {
    title: "Preferences",
    items: [
      { id: "preferences", label: "Preferences", icon: "pi pi-palette" },
      { id: "notifications", label: "Notifications", icon: "pi pi-bell" },
      { id: "advanced", label: "Advanced Settings", icon: "pi pi-cog" },
    ],
  },
  {
    title: "Help",
    items: [
      { id: "restore", label: "Restore Circle Access", icon: "pi pi-refresh" },
      { id: "about", label: "About XChat", icon: "pi pi-info-circle" },
    ],
  },
];

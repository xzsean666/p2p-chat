import type { SettingSection } from "../types/chat";

export const settingsSections: SettingSection[] = [
  {
    title: "Personal",
    items: [
      { id: "preferences", label: "Appearance & Chat", icon: "pi pi-palette" },
      { id: "notifications", label: "Notifications", icon: "pi pi-bell" },
    ],
  },
  {
    title: "Support",
    items: [
      { id: "restore", label: "Restore Access", icon: "pi pi-refresh" },
      { id: "about", label: "About XChat", icon: "pi pi-info-circle" },
    ],
  },
  {
    title: "Advanced",
    items: [{ id: "advanced", label: "Advanced", icon: "pi pi-cog" }],
  },
];

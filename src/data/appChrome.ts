import type { SettingSection, UserProfile } from "../types/chat";

export const userProfile: UserProfile = {
  name: "Sean Chen",
  handle: "@seanchen",
  initials: "SC",
  status: "Circle owner",
};

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
      { id: "restore", label: "Restore Purchases", icon: "pi pi-refresh" },
      { id: "about", label: "About XChat", icon: "pi pi-info-circle" },
    ],
  },
];

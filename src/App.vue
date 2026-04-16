<script setup lang="ts">
import ArchivedChatsPage from "./components/ArchivedChatsPage.vue";
import ChatPane from "./components/ChatPane.vue";
import CircleDirectoryPage from "./components/CircleDirectoryPage.vue";
import CircleDetailPage from "./components/CircleDetailPage.vue";
import CircleSwitcher from "./components/CircleSwitcher.vue";
import ContactProfilePage from "./components/ContactProfilePage.vue";
import ConversationDetailsDrawer from "./components/ConversationDetailsDrawer.vue";
import FindPeoplePage from "./components/FindPeoplePage.vue";
import GroupProfilePage from "./components/GroupProfilePage.vue";
import HomeTopBar from "./components/HomeTopBar.vue";
import LaunchScreen from "./components/LaunchScreen.vue";
import LoginScreen from "./components/LoginScreen.vue";
import NewMessagePage from "./components/NewMessagePage.vue";
import SettingsDetailPage from "./components/SettingsDetailPage.vue";
import SessionList from "./components/SessionList.vue";
import SettingsDrawer from "./components/SettingsDrawer.vue";
import { settingsSections, userProfile } from "./data/appChrome";
import { useChatShell } from "./features/shell/useChatShell";

const appVersion = "0.1.0";

const {
  searchText,
  composerText,
  isAuthenticated,
  showLaunch,
  showCircleSwitcher,
  showSettingsDrawer,
  showDetailsDrawer,
  circles,
  sessions,
  contacts,
  appPreferences,
  notificationPreferences,
  advancedPreferences,
  activeCircleId,
  selectedSessionId,
  bootstrapStatus,
  activeCircle,
  archivedSessionsForCircle,
  currentCircleContactIds,
  filteredSessions,
  selectedSession,
  activeMessages,
  selectedContact,
  selectedGroup,
  selectedGroupMembers,
  transportSnapshot,
  activeTransportDiagnostic,
  activeOverlayPage,
  activeOverlayContact,
  activeOverlayCircle,
  activeOverlayTransportDiagnostic,
  activeOverlayDiscoveredPeers,
  activeOverlaySessionSyncItems,
  activeOverlayTransportActivities,
  isActiveOverlayTransportBusy,
  activeOverlayCircleSessionCount,
  activeOverlayCircleDirectCount,
  activeOverlayCircleGroupCount,
  activeOverlayCircleArchivedCount,
  activeOverlayGroupSession,
  activeOverlayGroup,
  activeOverlayGroupMembers,
  inviteLink,
  selectSession,
  chooseCircle,
  toggleCircleSwitcher,
  openNewMessage,
  openFindPeoplePage,
  openCircleManagement,
  openCircleDetail,
  openDetailsDrawer,
  openContactProfile,
  openProfilePage,
  sendPreviewMessage,
  startConversation,
  handleSettingsAction,
  closeCircleOverlay,
  completeLogin,
  logout,
  handleSessionAction,
  openArchivedSession,
  toggleContactBlock,
  toggleGroupMute,
  leaveGroup,
  openMemberProfile,
  sendMessageFromProfile,
  openArchivedPage,
  addCircleFromDirectory,
  updateCircle,
  removeCircle,
  runTransportCircleAction,
  updateAppPreferences,
  updateNotificationPreferences,
  updateAdvancedPreferences,
  openCircleDirectoryFromSettings,
  closeTopOverlayPage,
} = useChatShell();
</script>

<template>
  <Transition name="launch-fade">
    <LaunchScreen v-if="showLaunch" />
  </Transition>

  <SettingsDrawer
    v-model:visible="showSettingsDrawer"
    :user="userProfile"
    :circles="circles"
    :active-circle-id="activeCircleId"
    :sections="settingsSections"
    :phase="bootstrapStatus?.phase"
    :show-logout="isAuthenticated"
    @select-circle="chooseCircle"
    @join-circle="openCircleManagement"
    @open-circle-detail="openCircleDetail"
    @item-click="handleSettingsAction"
    @logout="logout"
  />

  <ConversationDetailsDrawer
    v-model:visible="showDetailsDrawer"
    :session="selectedSession"
    :contact="selectedContact"
    :group="selectedGroup"
    :member-contacts="selectedGroupMembers"
    @toggle-block="toggleContactBlock"
    @toggle-mute-group="toggleGroupMute"
    @leave-group="leaveGroup"
  />

  <Transition name="page-sheet" mode="out-in">
    <CircleDirectoryPage
      v-if="activeOverlayPage?.kind === 'circle-directory'"
      key="circle-directory"
      :circles="circles"
      :active-circle-id="activeCircleId"
      :transport-snapshot="transportSnapshot"
      @close="closeTopOverlayPage"
      @select-circle="chooseCircle"
      @open-circle-detail="openCircleDetail"
      @add-circle="addCircleFromDirectory"
    />

    <CircleDetailPage
      v-else-if="activeOverlayPage?.kind === 'circle-detail'"
      :key="`circle-${activeOverlayPage.circleId}`"
      :circle="activeOverlayCircle"
      :is-active="activeOverlayCircle?.id === activeCircleId"
      :can-remove="circles.length > 1"
      :transport-diagnostic="activeOverlayTransportDiagnostic"
      :discovered-peers="activeOverlayDiscoveredPeers"
      :session-sync-items="activeOverlaySessionSyncItems"
      :transport-activities="activeOverlayTransportActivities"
      :transport-engine="transportSnapshot?.engine ?? null"
      :transport-busy="isActiveOverlayTransportBusy"
      :session-count="activeOverlayCircleSessionCount"
      :direct-count="activeOverlayCircleDirectCount"
      :group-count="activeOverlayCircleGroupCount"
      :archived-count="activeOverlayCircleArchivedCount"
      @close="closeTopOverlayPage"
      @select-circle="chooseCircle"
      @update-circle="updateCircle"
      @remove-circle="removeCircle"
      @transport-action="runTransportCircleAction"
    />

    <SettingsDetailPage
      v-else-if="activeOverlayPage?.kind === 'settings-detail'"
      :key="`settings-${activeOverlayPage.settingId}`"
      :setting-id="activeOverlayPage.settingId"
      :phase="bootstrapStatus?.phase"
      :version="appVersion"
      :active-circle="activeCircle"
      :circles-count="circles.length"
      :session-count="sessions.length"
      :preferences="appPreferences"
      :notifications="notificationPreferences"
      :advanced="advancedPreferences"
      :transport-snapshot="transportSnapshot"
      :active-transport-diagnostic="activeTransportDiagnostic"
      @close="closeTopOverlayPage"
      @open-circle-directory="openCircleDirectoryFromSettings"
      @update-preferences="updateAppPreferences"
      @update-notifications="updateNotificationPreferences"
      @update-advanced="updateAdvancedPreferences"
    />

    <NewMessagePage
      v-else-if="activeOverlayPage?.kind === 'new-message'"
      key="new-message"
      :contacts="contacts"
      :current-circle-contact-ids="currentCircleContactIds"
      :circle="activeCircle"
      :invite-link="inviteLink"
      @close="closeTopOverlayPage"
      @select-contact="startConversation"
      @open-find-people="openFindPeoplePage"
    />

    <FindPeoplePage
      v-else-if="activeOverlayPage?.kind === 'find-people'"
      key="find-people"
      :contacts="contacts"
      :current-circle-contact-ids="currentCircleContactIds"
      :circle="activeCircle"
      @close="closeTopOverlayPage"
      @open-contact="openContactProfile"
      @select-contact="startConversation"
    />

    <ArchivedChatsPage
      v-else-if="activeOverlayPage?.kind === 'archived'"
      key="archived"
      :sessions="archivedSessionsForCircle"
      @close="closeTopOverlayPage"
      @open-session="openArchivedSession"
      @unarchive-session="handleSessionAction({ sessionId: $event, action: 'unarchive' })"
    />

    <ContactProfilePage
      v-else-if="activeOverlayPage?.kind === 'contact'"
      :key="`contact-${activeOverlayPage.contactId}`"
      :contact="activeOverlayContact"
      @close="closeTopOverlayPage"
      @toggle-block="toggleContactBlock"
      @send-message="sendMessageFromProfile"
    />

    <GroupProfilePage
      v-else-if="activeOverlayPage?.kind === 'group'"
      :key="`group-${activeOverlayPage.sessionId}`"
      :session="activeOverlayGroupSession"
      :group="activeOverlayGroup"
      :member-contacts="activeOverlayGroupMembers"
      @close="closeTopOverlayPage"
      @open-member="openMemberProfile"
      @toggle-mute="toggleGroupMute"
      @leave-group="leaveGroup"
    />
  </Transition>

  <main class="app-shell">
    <template v-if="isAuthenticated">
      <HomeTopBar
        :user="userProfile"
        :circle="activeCircle"
        @avatar-click="showSettingsDrawer = true"
        @title-click="toggleCircleSwitcher"
        @add-click="openNewMessage"
      />

      <Transition name="overlay-fade">
        <div v-if="showCircleSwitcher" class="overlay-layer">
          <div class="overlay-mask" @click="closeCircleOverlay"></div>
          <div class="overlay-content">
            <CircleSwitcher
              :circles="circles"
              :active-circle-id="activeCircleId"
              @select="chooseCircle"
              @join="openCircleManagement"
            />
          </div>
        </div>
      </Transition>

      <section class="content-grid">
        <SessionList
          v-model:search-text="searchText"
          :sessions="filteredSessions"
          :active-session-id="selectedSessionId"
          :active-circle="activeCircle"
          :archived-count="archivedSessionsForCircle.length"
          @select-session="selectSession"
          @empty-action="openNewMessage"
          @session-action="handleSessionAction"
          @open-archived="openArchivedPage"
        />

        <ChatPane
          :session="selectedSession"
          :messages="activeMessages"
          :composer-text="composerText"
          @update:composer-text="composerText = $event"
          @send="sendPreviewMessage"
          @open-profile="openProfilePage"
          @open-details="openDetailsDrawer"
        />
      </section>
    </template>

    <LoginScreen
      v-else
      @quick-start="completeLogin"
      @existing-account="completeLogin"
      @signer-login="completeLogin"
    />
  </main>
</template>

<style>
:root {
  color: #172033;
  background:
    radial-gradient(circle at top left, rgba(95, 166, 255, 0.18), transparent 22%),
    radial-gradient(circle at right bottom, rgba(95, 216, 176, 0.16), transparent 18%),
    #eef3f7;
  font-family: "IBM Plex Sans", "Noto Sans SC", "Segoe UI", sans-serif;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

* {
  box-sizing: border-box;
}

html,
body,
#app {
  min-height: 100vh;
  margin: 0;
}

body {
  min-width: 360px;
}

button,
input,
textarea {
  font: inherit;
}

.app-shell {
  position: relative;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 16px;
  min-height: 100vh;
  padding: 18px;
}

.content-grid {
  display: grid;
  grid-template-columns: 360px minmax(0, 1fr);
  gap: 18px;
  min-height: 0;
}

.overlay-layer {
  position: fixed;
  inset: 0;
  z-index: 20;
}

.overlay-mask {
  position: absolute;
  inset: 0;
  background: rgba(17, 26, 41, 0.28);
  backdrop-filter: blur(4px);
}

.overlay-content {
  position: absolute;
  top: 86px;
  left: 50%;
  transform: translateX(-50%);
}

.launch-fade-enter-active,
.launch-fade-leave-active,
.overlay-fade-enter-active,
.overlay-fade-leave-active {
  transition: opacity 0.24s ease;
}

.page-sheet-enter-active,
.page-sheet-leave-active {
  transition:
    opacity 0.24s ease,
    transform 0.28s ease;
}

.launch-fade-enter-from,
.launch-fade-leave-to,
.overlay-fade-enter-from,
.overlay-fade-leave-to,
.page-sheet-enter-from,
.page-sheet-leave-to {
  opacity: 0;
}

.page-sheet-enter-from,
.page-sheet-leave-to {
  transform: translateX(24px);
}

@media (max-width: 1120px) {
  .content-grid {
    grid-template-columns: 320px minmax(0, 1fr);
  }
}

@media (max-width: 920px) {
  .content-grid {
    grid-template-columns: 1fr;
  }

  .overlay-content {
    top: 78px;
    width: 100%;
    padding: 0 12px;
  }
}

@media (max-width: 720px) {
  .app-shell {
    padding: 12px;
  }
}
</style>

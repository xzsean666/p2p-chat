<script setup lang="ts">
import Button from "primevue/button";
import ArchivedChatsPage from "./components/ArchivedChatsPage.vue";
import ChatPane from "./components/ChatPane.vue";
import CircleDirectoryPage from "./components/CircleDirectoryPage.vue";
import CircleDetailPage from "./components/CircleDetailPage.vue";
import CircleSwitcher from "./components/CircleSwitcher.vue";
import ContactProfilePage from "./components/ContactProfilePage.vue";
import ConversationDetailsDrawer from "./components/ConversationDetailsDrawer.vue";
import FindPeoplePage from "./components/FindPeoplePage.vue";
import GroupMembersManagePage from "./components/GroupMembersManagePage.vue";
import GroupMembersPage from "./components/GroupMembersPage.vue";
import GroupCreationPage from "./components/GroupCreationPage.vue";
import GroupNamePage from "./components/GroupNamePage.vue";
import GroupProfilePage from "./components/GroupProfilePage.vue";
import HomeTopBar from "./components/HomeTopBar.vue";
import LaunchScreen from "./components/LaunchScreen.vue";
import LoginScreen from "./components/LoginScreen.vue";
import NewMessagePage from "./components/NewMessagePage.vue";
import SelectGroupMembersPage from "./components/SelectGroupMembersPage.vue";
import SettingsDetailPage from "./components/SettingsDetailPage.vue";
import SessionList from "./components/SessionList.vue";
import SettingsDrawer from "./components/SettingsDrawer.vue";
import { settingsSections } from "./data/appChrome";
import { useChatShell } from "./features/shell/useChatShell";

const appVersion = "0.1.0";

const {
  searchText,
  composerText,
  isAuthenticated,
  userProfile,
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
  canLoadOlderMessages,
  loadingOlderMessages,
  selectedContact,
  selectedGroup,
  selectedGroupMembers,
  transportSnapshot,
  transportNotice,
  activeTransportDiagnostic,
  activeOverlayPage,
  activeOverlayContact,
  activeOverlayCircle,
  activeOverlayTransportDiagnostic,
  activeOverlayDiscoveredPeers,
  activeOverlaySessionSyncItems,
  activeOverlayTransportActivities,
  activeOverlayRuntimeSessions,
  isActiveOverlayTransportBusy,
  activeOverlayCircleSessionCount,
  activeOverlayCircleDirectCount,
  activeOverlayCircleGroupCount,
  activeOverlayCircleArchivedCount,
  activeOverlayGroupSession,
  activeOverlayGroup,
  activeOverlayGroupMembers,
  activeOverlayGroupAvailableContacts,
  activeOverlayGroupCreateContacts,
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
  openGroupSelectMembersPage,
  openGroupCreatePage,
  openProfilePage,
  updateComposerText,
  loadOlderMessages,
  sendPreviewMessage,
  retryMessageDelivery,
  startConversation,
  startSelfChat,
  createGroupChat,
  startLookupChat,
  joinCircleFromLookup,
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
  openGroupNamePage,
  openGroupMembersPage,
  openGroupAddMembersPage,
  openGroupRemoveMembersPage,
  updateGroupName,
  updateGroupMembers,
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
  dismissTransportNotice,
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
      :runtime-sessions="activeOverlayRuntimeSessions"
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
      @open-join-circle="openFindPeoplePage('join-circle')"
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
      @open-contact="openContactProfile"
      @select-contact="startConversation"
      @start-self="startSelfChat"
      @open-group-select="openGroupSelectMembersPage"
      @open-find-people="openFindPeoplePage"
    />

    <SelectGroupMembersPage
      v-else-if="activeOverlayPage?.kind === 'group-select-members'"
      key="group-select-members"
      :contacts="contacts"
      :current-circle-contact-ids="currentCircleContactIds"
      @close="closeTopOverlayPage"
      @open-contact="openContactProfile"
      @next="openGroupCreatePage"
    />

    <GroupCreationPage
      v-else-if="activeOverlayPage?.kind === 'group-create'"
      :key="`group-create-${activeOverlayPage.memberContactIds.join('-')}`"
      :circle="activeCircle"
      :user-profile="userProfile"
      :member-contacts="activeOverlayGroupCreateContacts"
      @close="closeTopOverlayPage"
      @open-member="openContactProfile"
      @create-group="createGroupChat"
    />

    <FindPeoplePage
      v-else-if="activeOverlayPage?.kind === 'find-people'"
      :key="`find-people-${activeOverlayPage.mode ?? 'chat'}`"
      :contacts="contacts"
      :current-circle-contact-ids="currentCircleContactIds"
      :circle="activeCircle"
      :mode="activeOverlayPage.mode ?? 'chat'"
      @close="closeTopOverlayPage"
      @open-contact="openContactProfile"
      @select-contact="startConversation"
      @lookup-contact="startLookupChat"
      @join-circle="joinCircleFromLookup"
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
      @open-members="openGroupMembersPage"
      @edit-name="openGroupNamePage"
      @add-members="openGroupAddMembersPage"
      @remove-members="openGroupRemoveMembersPage"
      @toggle-mute="toggleGroupMute"
      @leave-group="leaveGroup"
    />

    <GroupNamePage
      v-else-if="activeOverlayPage?.kind === 'group-name'"
      :key="`group-name-${activeOverlayPage.sessionId}`"
      :session="activeOverlayGroupSession"
      :group="activeOverlayGroup"
      @close="closeTopOverlayPage"
      @save="updateGroupName"
    />

    <GroupMembersPage
      v-else-if="activeOverlayPage?.kind === 'group-members'"
      :key="`group-members-${activeOverlayPage.sessionId}`"
      :session="activeOverlayGroupSession"
      :group="activeOverlayGroup"
      :member-contacts="activeOverlayGroupMembers"
      @close="closeTopOverlayPage"
      @open-member="openMemberProfile"
    />

    <GroupMembersManagePage
      v-else-if="activeOverlayPage?.kind === 'group-add-members'"
      :key="`group-add-members-${activeOverlayPage.sessionId}`"
      :session="activeOverlayGroupSession"
      :group="activeOverlayGroup"
      :member-contacts="activeOverlayGroupMembers"
      :candidate-contacts="activeOverlayGroupAvailableContacts"
      mode="add"
      @close="closeTopOverlayPage"
      @open-member="openMemberProfile"
      @save="updateGroupMembers"
    />

    <GroupMembersManagePage
      v-else-if="activeOverlayPage?.kind === 'group-remove-members'"
      :key="`group-remove-members-${activeOverlayPage.sessionId}`"
      :session="activeOverlayGroupSession"
      :group="activeOverlayGroup"
      :member-contacts="activeOverlayGroupMembers"
      :candidate-contacts="activeOverlayGroupAvailableContacts"
      mode="remove"
      @close="closeTopOverlayPage"
      @open-member="openMemberProfile"
      @save="updateGroupMembers"
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

      <Transition name="notice-slide">
        <section
          v-if="transportNotice"
          class="transport-notice"
          :class="`transport-notice-${transportNotice.tone}`"
          :role="transportNotice.tone === 'warn' ? 'alert' : 'status'"
          :aria-live="transportNotice.tone === 'warn' ? 'assertive' : 'polite'"
        >
          <div class="transport-notice-copy">
            <strong>{{ transportNotice.title }}</strong>
            <p>{{ transportNotice.detail }}</p>
          </div>

          <Button
            icon="pi pi-times"
            text
            rounded
            aria-label="Dismiss transport notice"
            class="transport-notice-dismiss"
            @click="dismissTransportNotice"
          />
        </section>
      </Transition>

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
          @empty-action="activeCircle?.type === 'paid' ? openFindPeoplePage('join-circle') : openFindPeoplePage('chat')"
          @session-action="handleSessionAction"
          @open-archived="openArchivedPage"
        />

        <ChatPane
          :session="selectedSession"
          :messages="activeMessages"
          :can-load-older-messages="canLoadOlderMessages"
          :loading-older-messages="loadingOlderMessages"
          :composer-text="composerText"
          @load-older="loadOlderMessages"
          @update:composer-text="updateComposerText"
          @send="sendPreviewMessage"
          @retry-message="retryMessageDelivery"
          @open-profile="openProfilePage"
          @open-details="openDetailsDrawer"
        />
      </section>
    </template>

    <LoginScreen
      v-else
      :circles="circles"
      :profile="userProfile"
      @complete="completeLogin"
    />
  </main>
</template>

<style>
:root {
  --shell-page-bg:
    radial-gradient(circle at top left, rgba(95, 166, 255, 0.18), transparent 22%),
    radial-gradient(circle at right bottom, rgba(95, 216, 176, 0.16), transparent 18%),
    #eef3f7;
  --shell-page-text: #172033;
  --shell-surface: rgba(255, 255, 255, 0.92);
  --shell-surface-strong: rgba(255, 255, 255, 0.97);
  --shell-surface-muted: #f7fafd;
  --shell-surface-soft: #f3f7fb;
  --shell-border: rgba(210, 220, 232, 0.9);
  --shell-border-soft: rgba(224, 232, 240, 0.9);
  --shell-hover: rgba(236, 243, 250, 0.92);
  --shell-selected: linear-gradient(135deg, #eff5ff 0%, #eefaf5 100%);
  --shell-selected-border: rgba(170, 198, 228, 0.92);
  --shell-text-strong: #18253d;
  --shell-text-default: #31425e;
  --shell-text-muted: #6c7f98;
  --shell-text-soft: #7a8ca3;
  --shell-avatar-bg: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  --shell-avatar-text: #16355c;
  --shell-shadow-soft: 0 20px 50px rgba(24, 46, 84, 0.08);
  --shell-shadow-strong: 0 26px 64px rgba(17, 36, 66, 0.2);
  font-family: "IBM Plex Sans", "Noto Sans SC", "Segoe UI", sans-serif;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

html[data-shell-theme="light"] {
  color-scheme: light;
}

html[data-shell-theme="ink"] {
  color-scheme: dark;
  --shell-page-bg:
    radial-gradient(circle at top left, rgba(91, 138, 211, 0.18), transparent 24%),
    radial-gradient(circle at right bottom, rgba(66, 154, 127, 0.16), transparent 22%),
    #0f1726;
  --shell-page-text: #e8eef8;
  --shell-surface: rgba(18, 28, 44, 0.9);
  --shell-surface-strong: rgba(18, 28, 44, 0.96);
  --shell-surface-muted: rgba(28, 40, 60, 0.92);
  --shell-surface-soft: rgba(26, 37, 57, 0.88);
  --shell-border: rgba(79, 101, 135, 0.44);
  --shell-border-soft: rgba(79, 101, 135, 0.32);
  --shell-hover: rgba(39, 54, 79, 0.92);
  --shell-selected: linear-gradient(135deg, rgba(46, 73, 110, 0.9) 0%, rgba(27, 82, 72, 0.82) 100%);
  --shell-selected-border: rgba(103, 146, 205, 0.46);
  --shell-text-strong: #f1f6ff;
  --shell-text-default: #d7e1f1;
  --shell-text-muted: #97a8c4;
  --shell-text-soft: #8a9bb6;
  --shell-avatar-bg: linear-gradient(135deg, rgba(68, 104, 160, 0.88) 0%, rgba(43, 117, 99, 0.82) 100%);
  --shell-avatar-text: #f4f8ff;
  --shell-shadow-soft: 0 22px 52px rgba(4, 10, 20, 0.34);
  --shell-shadow-strong: 0 28px 70px rgba(2, 8, 18, 0.45);
}

html[data-shell-text-size="compact"] {
  font-size: 15px;
}

html[data-shell-text-size="default"] {
  font-size: 16px;
}

html[data-shell-text-size="large"] {
  font-size: 17.5px;
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
  color: var(--shell-page-text);
  background: var(--shell-page-bg);
  transition:
    background 0.28s ease,
    color 0.2s ease,
    font-size 0.2s ease;
}

button,
input,
textarea {
  font: inherit;
  color: inherit;
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

.notice-slide-enter-active,
.notice-slide-leave-active {
  transition:
    opacity 0.24s ease,
    transform 0.24s ease;
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

.notice-slide-enter-from,
.notice-slide-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

.transport-notice {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: start;
  gap: 12px;
  padding: 14px 16px 14px 18px;
  border-radius: 22px;
  border: 1px solid var(--shell-border);
  background: color-mix(in srgb, var(--shell-surface-strong) 96%, transparent);
  box-shadow: var(--shell-shadow-soft);
}

.transport-notice-warn {
  border-color: rgba(225, 154, 110, 0.46);
  background:
    linear-gradient(135deg, rgba(255, 247, 238, 0.98), rgba(255, 255, 255, 0.94)),
    rgba(255, 255, 255, 0.94);
}

.transport-notice-info {
  border-color: rgba(95, 166, 255, 0.34);
  background:
    linear-gradient(135deg, rgba(239, 247, 255, 0.98), rgba(255, 255, 255, 0.94)),
    rgba(255, 255, 255, 0.94);
}

.transport-notice-copy {
  min-width: 0;
  display: grid;
  gap: 4px;
}

.transport-notice-copy strong {
  color: var(--shell-text-strong);
  font-size: 0.95rem;
}

.transport-notice-copy p {
  margin: 0;
  color: var(--shell-text-muted);
  font-size: 0.88rem;
  line-height: 1.45;
}

.transport-notice-dismiss {
  margin-top: -2px;
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

  .transport-notice {
    grid-template-columns: 1fr;
  }

  .transport-notice-dismiss {
    justify-self: end;
  }
}
</style>

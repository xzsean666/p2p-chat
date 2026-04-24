<script setup lang="ts">
import { ref, watch } from "vue";
import Button from "primevue/button";
import ArchivedChatsPage from "./components/ArchivedChatsPage.vue";
import ChatPane from "./components/ChatPane.vue";
import CircleDirectoryPage from "./components/CircleDirectoryPage.vue";
import CircleDetailPage from "./components/CircleDetailPage.vue";
import CircleInvitePage from "./components/CircleInvitePage.vue";
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
import MessageDetailPage from "./components/MessageDetailPage.vue";
import NewMessagePage from "./components/NewMessagePage.vue";
import SelfChatConfirmPage from "./components/SelfChatConfirmPage.vue";
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
  findPeopleSubmitting,
  findPeopleErrorMessage,
  isAuthenticated,
  authSession,
  authRuntime,
  authRuntimeBinding,
  userProfile,
  restorableCircles,
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
  replyingToMessage,
  mentionSuggestions,
  showMentionSuggestions,
  mentionSelectionIndex,
  canLoadOlderMessages,
  loadingOlderMessages,
  selectedContact,
  selectedGroup,
  selectedGroupMembers,
  transportSnapshot,
  transportNotice,
  canSendMessages,
  sendBlockedReason,
  runtimeDiagnosticError,
  activeTransportDiagnostic,
  activeOverlayPage,
  activeOverlayContact,
  activeOverlayMessageSession,
  activeOverlayMessage,
  activeOverlayMessageReplyTarget,
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
  openCircleInvitePage,
  openSelfChatConfirmPage,
  openFindPeoplePage,
  openCircleManagement,
  openCircleDetail,
  openContactProfile,
  openMessageDetailPage,
  openGroupSelectMembersPage,
  openGroupCreatePage,
  openProfilePage,
  updateComposerText,
  navigateMentionSuggestions,
  selectMentionSuggestion,
  startReplyToMessage,
  cancelReplyToMessage,
  copyMessageContent,
  copyMessageAttachmentPath,
  openMessageAttachment,
  revealMessageAttachment,
  reportMessage,
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
  updateAuthRuntime,
  syncAuthRuntimeNow,
  handleSessionAction,
  openArchivedSession,
  toggleContactBlock,
  updateContactRemark,
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
  restoreCircleAccess,
  forgetRestorableCircle,
  runTransportCircleAction,
  updateAppPreferences,
  updateNotificationPreferences,
  updateAdvancedPreferences,
  openCircleDirectoryFromSettings,
  closeTopOverlayPage,
  dismissTransportNotice,
} = useChatShell();

const showSessionChatPage = ref(false);

watch(
  isAuthenticated,
  (authenticated) => {
    if (!authenticated) {
      showSessionChatPage.value = false;
    }
  },
  { immediate: true },
);

watch(selectedSessionId, (nextSessionId, previousSessionId) => {
  if (!isAuthenticated.value) {
    return;
  }

  if (!nextSessionId) {
    showSessionChatPage.value = false;
    return;
  }

  if (
    nextSessionId !== previousSessionId &&
    (showSessionChatPage.value || !!activeOverlayPage.value)
  ) {
    showSessionChatPage.value = true;
  }
});

watch(activeCircleId, () => {
  showSessionChatPage.value = false;
});

function openSessionChatPage(sessionId: string) {
  selectSession(sessionId);
  showSessionChatPage.value = true;
}

function closeSessionChatPage() {
  showSessionChatPage.value = false;
}

async function shareCircleInvite() {
  const url = inviteLink.value.trim();
  if (!url) {
    return;
  }

  const share = (navigator as Navigator & {
    share?: (data: { title?: string; text?: string; url?: string }) => Promise<void>;
  }).share;

  if (typeof share === "function") {
    try {
      await share({
        title: activeCircle.value?.name ?? "Circle Invite",
        text: activeCircle.value ? `Invite to ${activeCircle.value.name}` : "Circle invite",
        url,
      });
      return;
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") {
        return;
      }
    }
  }

  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(url);
    } catch {
      // The invite page keeps its own copy affordance; this fallback stays silent.
    }
  }
}
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
    :restorable-count="restorableCircles.length"
    :sections="settingsSections"
    :phase="bootstrapStatus?.phase"
    :show-logout="isAuthenticated"
    @select-circle="chooseCircle"
    @join-circle="openCircleManagement"
    @open-restore="handleSettingsAction('restore')"
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
      :restorable-circles="restorableCircles"
      :transport-snapshot="transportSnapshot"
      @close="closeTopOverlayPage"
      @select-circle="chooseCircle"
      @open-circle-detail="openCircleDetail"
      @add-circle="addCircleFromDirectory"
      @restore-circle="restoreCircleAccess"
      @forget-restorable-circle="forgetRestorableCircle"
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
      :auth-session="authSession"
      :auth-runtime="authRuntime"
      :auth-runtime-binding="authRuntimeBinding"
      :active-circle="activeCircle"
      :circles-count="circles.length"
      :restorable-circles="restorableCircles"
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
      @update-auth-runtime="updateAuthRuntime"
      @sync-auth-runtime="syncAuthRuntimeNow"
      @restore-circle="restoreCircleAccess"
      @forget-restorable-circle="forgetRestorableCircle"
    />

    <NewMessagePage
      v-else-if="activeOverlayPage?.kind === 'new-message'"
      key="new-message"
      :contacts="contacts"
      :current-circle-contact-ids="currentCircleContactIds"
      :circle="activeCircle"
      :invite-link="inviteLink"
      :can-invite="!!activeCircle"
      :can-add-friends="!!activeCircle"
      @close="closeTopOverlayPage"
      @open-contact="openContactProfile"
      @select-contact="startConversation"
      @open-self-confirm="openSelfChatConfirmPage"
      @open-group-select="openGroupSelectMembersPage"
      @open-find-people="openFindPeoplePage"
      @open-invite="openCircleInvitePage"
    />

    <CircleInvitePage
      v-else-if="activeOverlayPage?.kind === 'circle-invite'"
      key="circle-invite"
      :circle-name="activeCircle?.name ?? 'Circle Invite'"
      :invite-link="inviteLink"
      @close="closeTopOverlayPage"
      @share="shareCircleInvite"
    />

    <SelfChatConfirmPage
      v-else-if="activeOverlayPage?.kind === 'self-chat-confirm'"
      key="self-chat-confirm"
      :circle="activeCircle"
      @close="closeTopOverlayPage"
      @confirm="startSelfChat"
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
      :submitting="findPeopleSubmitting"
      :submit-error="findPeopleErrorMessage"
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
      :active-circle="activeCircle"
      @close="closeTopOverlayPage"
      @open-join-circle="openFindPeoplePage('join-circle')"
      @toggle-block="toggleContactBlock"
      @save-remark="updateContactRemark"
      @send-message="sendMessageFromProfile"
    />

    <MessageDetailPage
      v-else-if="activeOverlayPage?.kind === 'message-detail'"
      :key="`message-${activeOverlayPage.sessionId}-${activeOverlayPage.messageId}`"
      :session="activeOverlayMessageSession"
      :message="activeOverlayMessage"
      :replied-message="activeOverlayMessageReplyTarget"
      :can-send-messages="canSendMessages"
      @close="closeTopOverlayPage"
      @retry-message="retryMessageDelivery"
      @open-attachment="openMessageAttachment($event, activeOverlayPage.sessionId)"
      @open-replied-message="openMessageDetailPage($event, activeOverlayPage.sessionId)"
      @reveal-attachment="revealMessageAttachment($event, activeOverlayPage.sessionId)"
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

    <template v-if="isAuthenticated">
      <section
        class="authenticated-shell"
        :class="{ 'authenticated-shell-stacked': showSessionChatPage || !!activeOverlayPage }"
      >
        <section
          class="primary-page"
          :aria-hidden="showSessionChatPage || !!activeOverlayPage ? 'true' : 'false'"
        >
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
                  @restore="handleSettingsAction('restore')"
                />
              </div>
            </div>
          </Transition>

          <section class="content-list">
            <SessionList
              v-model:search-text="searchText"
              :sessions="filteredSessions"
              :active-session-id="selectedSessionId"
              :active-circle="activeCircle"
              :archived-count="archivedSessionsForCircle.length"
              @select-session="openSessionChatPage"
              @empty-action="!activeCircle || activeCircle.type === 'paid' ? openFindPeoplePage('join-circle') : openFindPeoplePage('chat')"
              @session-action="handleSessionAction"
              @open-archived="openArchivedPage"
            />
          </section>
        </section>
      </section>
    </template>

    <LoginScreen
      v-else
      :circles="circles"
      :restorable-circles="restorableCircles"
      :profile="userProfile"
      @complete="completeLogin"
    />

    <Transition name="page-sheet">
      <section
        v-if="isAuthenticated && showSessionChatPage && selectedSession"
        class="chat-page-layer"
      >
        <ChatPane
          presentation="page"
          :show-back-button="true"
          :session="selectedSession"
          :active-circle="activeCircle"
          :messages="activeMessages"
          :replying-to-message="replyingToMessage"
          :mention-suggestions="mentionSuggestions"
          :show-mention-suggestions="showMentionSuggestions"
          :mention-selection-index="mentionSelectionIndex"
          :can-load-older-messages="canLoadOlderMessages"
          :loading-older-messages="loadingOlderMessages"
          :composer-text="composerText"
          :show-message-info="advancedPreferences.showMessageInfo"
          :can-send-messages="canSendMessages"
          :send-blocked-reason="sendBlockedReason"
          :runtime-error="runtimeDiagnosticError"
          @back="closeSessionChatPage"
          @load-older="loadOlderMessages"
          @update:composer-text="updateComposerText"
          @mention-navigate="navigateMentionSuggestions"
          @mention-select="selectMentionSuggestion"
          @reply-message="startReplyToMessage"
          @cancel-reply="cancelReplyToMessage"
          @open-message-detail="openMessageDetailPage"
          @copy-message="copyMessageContent"
          @copy-attachment-path="copyMessageAttachmentPath"
          @open-attachment="openMessageAttachment"
          @report-message="reportMessage($event.messageId, $event.reason)"
          @reveal-attachment="revealMessageAttachment"
          @send="sendPreviewMessage"
          @retry-message="retryMessageDelivery"
          @open-profile="openProfilePage"
          @open-details="openProfilePage"
        />
      </section>
    </Transition>
  </main>
</template>

<style>
:root {
  --shell-page-bg:
    radial-gradient(circle at top left, rgba(219, 230, 241, 0.72), transparent 18%),
    radial-gradient(circle at right bottom, rgba(232, 238, 244, 0.78), transparent 22%),
    #f6f7f9;
  --shell-page-text: #1d1c21;
  --shell-surface: rgba(255, 255, 255, 0.94);
  --shell-surface-strong: rgba(255, 255, 255, 0.99);
  --shell-surface-muted: #f7f8fa;
  --shell-surface-soft: #eef2f5;
  --shell-border: rgba(215, 220, 228, 0.92);
  --shell-border-soft: rgba(228, 232, 237, 0.9);
  --shell-hover: rgba(44, 108, 181, 0.06);
  --shell-selected: linear-gradient(180deg, rgba(44, 108, 181, 0.1), rgba(44, 108, 181, 0.03));
  --shell-selected-border: rgba(44, 108, 181, 0.18);
  --shell-text-strong: #1a1b1e;
  --shell-text-default: #404753;
  --shell-text-muted: #7a8593;
  --shell-text-soft: #95a0ad;
  --shell-accent: #2c6cb5;
  --shell-accent-soft: rgba(44, 108, 181, 0.12);
  --shell-danger: #ff6767;
  --shell-success: #4caf7a;
  --shell-avatar-bg: linear-gradient(135deg, rgba(44, 108, 181, 0.12), rgba(44, 108, 181, 0.18));
  --shell-avatar-text: #29568c;
  --shell-shadow-soft: 0 18px 42px rgba(35, 47, 65, 0.06);
  --shell-shadow-strong: 0 28px 68px rgba(35, 47, 65, 0.14);
  font-family: "Lato", "Noto Sans", "Noto Sans SC", "Segoe UI", sans-serif;
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
    radial-gradient(circle at top left, rgba(111, 97, 232, 0.2), transparent 24%),
    radial-gradient(circle at right bottom, rgba(143, 130, 255, 0.18), transparent 24%),
    #1f1c38;
  --shell-page-text: #f5f5f7;
  --shell-surface: rgba(34, 29, 62, 0.92);
  --shell-surface-strong: rgba(34, 29, 62, 0.97);
  --shell-surface-muted: rgba(43, 34, 80, 0.96);
  --shell-surface-soft: rgba(53, 45, 96, 0.84);
  --shell-border: rgba(126, 116, 194, 0.42);
  --shell-border-soft: rgba(126, 116, 194, 0.28);
  --shell-hover: rgba(111, 97, 232, 0.16);
  --shell-selected: linear-gradient(180deg, rgba(111, 97, 232, 0.3), rgba(111, 97, 232, 0.14));
  --shell-selected-border: rgba(156, 145, 242, 0.42);
  --shell-text-strong: #ffffff;
  --shell-text-default: #ece9ff;
  --shell-text-muted: #b5aed7;
  --shell-text-soft: #9d96c2;
  --shell-accent: #8e81ff;
  --shell-accent-soft: rgba(142, 129, 255, 0.16);
  --shell-danger: #ff7c7c;
  --shell-success: #7edca7;
  --shell-avatar-bg: linear-gradient(135deg, rgba(142, 129, 255, 0.28), rgba(111, 97, 232, 0.44));
  --shell-avatar-text: #f7f5ff;
  --shell-shadow-soft: 0 22px 54px rgba(10, 8, 24, 0.34);
  --shell-shadow-strong: 0 30px 78px rgba(8, 7, 21, 0.5);
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
  grid-template-rows: minmax(0, 1fr);
  min-height: 100vh;
  overflow: hidden;
}

.authenticated-shell,
.primary-page {
  min-height: 0;
}

.authenticated-shell {
  position: relative;
  display: grid;
  grid-template-rows: minmax(0, 1fr);
  height: 100vh;
  background: var(--shell-page-bg);
}

.primary-page {
  position: relative;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 10px;
  padding: 12px 14px 14px;
  transition:
    transform 0.3s cubic-bezier(0.22, 1, 0.36, 1),
    opacity 0.24s ease,
    filter 0.3s ease;
  will-change: transform, opacity;
}

.authenticated-shell-stacked .primary-page {
  transform: translateX(-10%) scale(0.985);
  opacity: 0.9;
  filter: saturate(0.92);
  pointer-events: none;
}

.content-grid {
  display: grid;
  grid-template-columns: 340px minmax(0, 1fr);
  gap: 14px;
  min-height: 0;
}

.content-list {
  display: grid;
  min-height: 0;
  max-width: min(100%, 560px);
}

.overlay-layer {
  position: fixed;
  inset: 0;
  z-index: 18;
  display: grid;
  align-items: start;
  padding-top: 74px;
}

.chat-page-layer {
  position: fixed;
  inset: 0;
  z-index: 24;
  background: var(--shell-surface-strong);
}

.overlay-mask {
  position: absolute;
  inset: 0;
  background: rgba(14, 16, 21, 0.2);
}

.overlay-content {
  position: relative;
  z-index: 1;
  display: flex;
  justify-content: center;
  width: 100%;
  padding: 0 12px;
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
    opacity 0.22s ease,
    transform 0.34s cubic-bezier(0.22, 1, 0.36, 1);
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
  transform: translateX(100%);
}

.notice-slide-enter-from,
.notice-slide-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

.transport-notice {
  position: fixed;
  top: 12px;
  left: 14px;
  right: 14px;
  z-index: 32;
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
  .content-list {
    max-width: none;
  }

  .primary-page {
    padding: 12px 12px 12px;
  }

  .overlay-content {
    padding: 0 10px;
  }
}

@media (max-width: 720px) {
  .primary-page {
    gap: 8px;
    padding: 10px 10px calc(12px + env(safe-area-inset-bottom));
  }

  .authenticated-shell-stacked .primary-page {
    transform: translateX(-18%) scale(0.985);
  }

  .transport-notice {
    grid-template-columns: 1fr;
    left: 10px;
    right: 10px;
  }

  .transport-notice-dismiss {
    justify-self: end;
  }
}
</style>

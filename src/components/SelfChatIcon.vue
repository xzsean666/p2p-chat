<script setup lang="ts">
withDefaults(
  defineProps<{
    size?: "md" | "lg";
  }>(),
  {
    size: "md",
  },
);
</script>

<template>
  <div :class="['self-chat-icon', size]">
    <span></span>
    <span></span>
    <span></span>
    <span></span>
  </div>
</template>

<style scoped>
.self-chat-icon {
  display: grid;
  align-content: center;
  gap: 3px;
  border-radius: 999px;
  background: #ffe7e6;
}

.self-chat-icon.md {
  width: 44px;
  height: 44px;
  padding: 10px 12px;
}

.self-chat-icon.lg {
  width: 48px;
  height: 48px;
  padding: 11px 13px;
}

.self-chat-icon span {
  display: block;
  height: 3px;
  border-radius: 999px;
  background: #d84645;
}

.self-chat-icon span:nth-child(1) {
  width: 42%;
}

.self-chat-icon span:nth-child(2) {
  width: 72%;
}

.self-chat-icon span:nth-child(3) {
  width: 62%;
}

.self-chat-icon span:nth-child(4) {
  width: 68%;
}
</style>

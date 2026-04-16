<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";

const emit = defineEmits<{
  (event: "quick-start"): void;
  (event: "existing-account"): void;
  (event: "signer-login"): void;
}>();

const slides = [
  {
    title: "Private messenger based on circles",
    text: "Connect with people through encrypted chats, private relays and portable identity.",
  },
  {
    title: "Relay switching stays in the core flow",
    text: "Move between circles without losing the session context or the message-driven layout.",
  },
  {
    title: "Profiles, groups and settings are first-class pages",
    text: "The desktop rebuild follows the same interaction rhythm instead of flattening everything into one view.",
  },
];

const currentSlide = ref(0);
let timer: number | undefined;

onMounted(() => {
  timer = window.setInterval(() => {
    currentSlide.value = (currentSlide.value + 1) % slides.length;
  }, 3200);
});

onBeforeUnmount(() => {
  if (timer) {
    window.clearInterval(timer);
  }
});
</script>

<template>
  <section class="login-screen">
    <div class="login-card">
      <div class="login-hero">
        <p class="eyebrow">Welcome to XChat</p>
        <h1>{{ slides[currentSlide].title }}</h1>
        <p class="hero-copy">{{ slides[currentSlide].text }}</p>

        <div class="slide-markers">
          <button
            v-for="(_, index) in slides"
            :key="index"
            type="button"
            :class="['marker', { active: index === currentSlide }]"
            @click="currentSlide = index"
          />
        </div>
      </div>

      <div class="login-actions">
        <Button
          label="Get Started"
          severity="contrast"
          class="action-button"
          @click="emit('quick-start')"
        />
        <Button
          label="I Have a Nostr Account"
          text
          severity="contrast"
          class="action-button"
          @click="emit('existing-account')"
        />
        <Button
          icon="pi pi-key"
          label="Login With Signer"
          text
          severity="secondary"
          class="action-button signer-button"
          @click="emit('signer-login')"
        />

        <div class="agreement-row">
          <Checkbox :binary="true" :model-value="true" readonly />
          <p>
            By continuing you agree to the privacy policy and terms of service.
          </p>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.login-screen {
  display: grid;
  place-items: center;
  min-height: calc(100vh - 36px);
}

.login-card {
  display: grid;
  grid-template-columns: minmax(0, 1.2fr) minmax(320px, 0.9fr);
  gap: 20px;
  width: min(1120px, calc(100vw - 36px));
  min-height: min(720px, calc(100vh - 36px));
  padding: 22px;
  border-radius: 32px;
  background: rgba(255, 255, 255, 0.92);
  border: 1px solid rgba(210, 220, 232, 0.9);
  box-shadow: 0 24px 60px rgba(24, 46, 84, 0.1);
}

.login-hero,
.login-actions {
  border-radius: 28px;
}

.login-hero {
  display: grid;
  align-content: end;
  gap: 16px;
  padding: 34px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.28), transparent 24%),
    radial-gradient(circle at right bottom, rgba(76, 215, 166, 0.24), transparent 20%),
    linear-gradient(180deg, #233966 0%, #1a2947 100%);
  color: #f5f8fe;
}

.eyebrow,
.hero-copy,
.agreement-row p {
  margin: 0;
}

.eyebrow {
  text-transform: uppercase;
  letter-spacing: 0.18em;
  font-size: 0.76rem;
  color: rgba(245, 248, 254, 0.74);
}

.login-hero h1 {
  margin: 0;
  font-size: clamp(2.4rem, 5vw, 4.2rem);
  line-height: 0.98;
  letter-spacing: -0.05em;
  max-width: 10ch;
}

.hero-copy {
  max-width: 44ch;
  color: rgba(245, 248, 254, 0.84);
  line-height: 1.7;
}

.slide-markers {
  display: flex;
  gap: 8px;
}

.marker {
  width: 34px;
  height: 6px;
  border: 0;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.22);
  cursor: pointer;
}

.marker.active {
  background: #ffffff;
}

.login-actions {
  display: grid;
  align-content: center;
  gap: 14px;
  padding: 28px;
  background: linear-gradient(180deg, #f8fbfe 0%, #f2f7fb 100%);
}

.action-button {
  width: 100%;
}

.signer-button {
  margin-top: 8px;
}

.agreement-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 10px;
  align-items: start;
  margin-top: 10px;
  color: #6c8098;
  font-size: 0.9rem;
  line-height: 1.6;
}

@media (max-width: 920px) {
  .login-card {
    grid-template-columns: 1fr;
    min-height: auto;
  }

  .login-screen {
    min-height: calc(100vh - 24px);
  }
}
</style>

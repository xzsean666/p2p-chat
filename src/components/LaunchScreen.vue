<template>
  <section class="launch-screen">
    <div class="launch-card">
      <div class="launch-mark">
        <span class="mark-dot"></span>
        <span class="mark-dot"></span>
        <span class="mark-dot"></span>
      </div>
      <p class="eyebrow">Desktop Rebuild</p>
      <h1>P2P Chat</h1>
      <p class="summary">Rust + Tauri 2 + Vue + PrimeVue</p>
    </div>
  </section>
</template>

<style scoped>
.launch-screen {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: grid;
  place-items: center;
  background:
    radial-gradient(circle at top left, rgba(97, 169, 255, 0.24), transparent 26%),
    radial-gradient(circle at right bottom, rgba(68, 208, 159, 0.18), transparent 18%),
    linear-gradient(180deg, #f4f7fb 0%, #edf3f9 100%);
}

.launch-card {
  display: grid;
  justify-items: center;
  gap: 10px;
  padding: 34px 40px;
  border-radius: 28px;
  background: rgba(255, 255, 255, 0.92);
  box-shadow: 0 20px 40px rgba(28, 48, 86, 0.08);
}

.launch-mark {
  display: flex;
  gap: 8px;
}

.mark-dot {
  width: 12px;
  height: 12px;
  border-radius: 999px;
  background: linear-gradient(135deg, #6aa8ff 0%, #55d9ae 100%);
  animation: pulse 1.2s ease-in-out infinite;
}

.mark-dot:nth-child(2) {
  animation-delay: 0.15s;
}

.mark-dot:nth-child(3) {
  animation-delay: 0.3s;
}

.eyebrow,
.summary {
  margin: 0;
  color: #60728f;
}

.eyebrow {
  letter-spacing: 0.16em;
  text-transform: uppercase;
  font-size: 0.76rem;
}

h1 {
  margin: 0;
  font-size: 2.5rem;
  letter-spacing: -0.04em;
}

@keyframes pulse {
  0%,
  100% {
    transform: scale(0.85);
    opacity: 0.5;
  }

  50% {
    transform: scale(1);
    opacity: 1;
  }
}
</style>

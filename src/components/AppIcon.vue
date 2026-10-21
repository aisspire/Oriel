<script setup lang="ts">
defineProps<{
  name: 'home' | 'sun' | 'moon' | 'pause' | 'play' | 'minimize' | 'maximize' | 'restore' | 'close' | 'globe' | 'back' | 'forward' | 'refresh' | 'key' | 'settings' | 'plus'
}>()
</script>

<template>
  <svg
    viewBox="0 0 24 24"
    width="24"
    height="24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.6"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <template v-if="name === 'globe'">
      <circle cx="12" cy="12" r="9" /><ellipse cx="12" cy="12" rx="4" ry="9" /><path d="M3 12h18" />
    </template>
    <path v-else-if="name === 'back'" d="m14 6-6 6 6 6" />
    <path v-else-if="name === 'forward'" d="m10 6 6 6-6 6" />
    <template v-else-if="name === 'refresh'"><path d="M20 7v5h-5M20 12a8 8 0 1 0-2.3 5.7" /></template>
    <template v-else-if="name === 'key'"><circle cx="8" cy="8" r="4.5" /><path d="m11.3 11.3 9.2 9.2m-3-3 3-3m-6 0 3-3" /></template>
    <template v-else-if="name === 'settings'"><path d="M4 7h16M4 17h16" /><circle cx="9" cy="7" r="2.5" fill="var(--shell)" /><circle cx="15" cy="17" r="2.5" fill="var(--shell)" /></template>
    <path v-else-if="name === 'plus'" d="M12 5v14M5 12h14" />
    <template v-else-if="name === 'home'">
      <path d="m3.5 10 7-6a2.3 2.3 0 0 1 3 0l7 6" />
      <path d="M5.5 8.5V19a1.5 1.5 0 0 0 1.5 1.5h10a1.5 1.5 0 0 0 1.5-1.5V8.5" />
      <path d="M9.5 20.5v-7h5v7" />
    </template>
    <template v-else-if="name === 'sun'">
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2m0 16v2M2 12h2m16 0h2M4.9 4.9l1.4 1.4m11.4 11.4 1.4 1.4m0-14.2-1.4 1.4M6.3 17.7l-1.4 1.4" />
    </template>
    <path
      v-else-if="name === 'moon'"
      d="M20.5 13a8.5 8.5 0 0 1-9.5-9.5A8.5 8.5 0 1 0 20.5 13Z"
    />
    <template v-else-if="name === 'pause'">
      <path d="M9 6v12m6-12v12" stroke-width="2.5" />
    </template>
    <path v-else-if="name === 'play'" d="m9 5 10 7-10 7V5Z" />
    <path v-else-if="name === 'minimize'" d="M5 12h14" />
    <rect v-else-if="name === 'maximize'" x="5" y="5" width="14" height="14" rx=".5" />
    <template v-else-if="name === 'restore'">
      <path d="M9 7V4h11v11h-3" />
      <rect x="4" y="9" width="11" height="11" rx=".5" />
    </template>
    <path v-else-if="name === 'close'" d="m6 6 12 12M18 6 6 18" />
  </svg>
</template>

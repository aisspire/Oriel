<script setup lang="ts">
defineProps<{
  name: 'home' | 'sun' | 'moon' | 'pause' | 'play' | 'window' | 'spark'
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
    <template v-if="name === 'home'">
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
    <template v-else-if="name === 'window'">
      <rect x="4" y="3" width="16" height="18" rx="5" />
      <path d="M12 3v18M4 12h16" />
    </template>
    <path v-else-if="name === 'spark'" d="M12 3c0 5-4 9-9 9 5 0 9 4 9 9 0-5 4-9 9-9-5 0-9-4-9-9Z" />
  </svg>
</template>

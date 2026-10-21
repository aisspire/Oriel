<script setup lang="ts">
defineProps<{ paused: boolean }>()
</script>

<template>
  <section class="showcase" :class="{ 'is-paused': paused }" aria-labelledby="showcase-title">
    <div class="wordmark-atmosphere" aria-hidden="true">
      <div class="wordmark-aura aura-mint"></div>
      <div class="wordmark-aura aura-peach"></div>
      <div class="wordmark-aura aura-blue"></div>
    </div>
    <h1 id="showcase-title" class="oriel-wordmark">Oriel</h1>
  </section>
</template>

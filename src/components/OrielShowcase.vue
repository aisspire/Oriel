<script setup lang="ts">
import AppIcon from './AppIcon.vue'

defineProps<{
  paused: boolean
  reducedMotion: boolean
}>()

defineEmits<{
  toggleMotion: []
}>()
</script>

<template>
  <section
    class="showcase panel"
    :class="{ 'is-paused': paused }"
    aria-labelledby="showcase-title"
  >
    <div class="showcase-topline">
      <span class="showcase-eyebrow">
        <span class="small-cross" aria-hidden="true"></span>
        A WINDOW TO YOUR WORLD
      </span>
      <button
        v-if="!reducedMotion"
        class="icon-button motion-toggle"
        type="button"
        :aria-label="paused ? '播放首页动效' : '暂停首页动效'"
        :title="paused ? '播放动效' : '暂停动效'"
        @click="$emit('toggleMotion')"
      >
        <AppIcon :name="paused ? 'play' : 'pause'" />
      </button>
      <span v-else class="motion-note">静态展示</span>
    </div>

    <div class="oriel-scene" aria-hidden="true">
      <div class="scene-grid"></div>
      <div class="scene-aura aura-mint"></div>
      <div class="scene-aura aura-peach"></div>
      <div class="scene-aura aura-blue"></div>
      <div class="window-sculpture">
        <div class="window-plane plane-back"></div>
        <div class="window-plane plane-middle"></div>
        <div class="window-plane plane-front">
          <span class="plane-corner corner-top"></span>
          <span class="plane-corner corner-bottom"></span>
        </div>
        <div class="floating-logo">
          <img src="/oriel-icon.svg" alt="" width="512" height="512" draggable="false" />
          <div class="logo-reflection"></div>
        </div>
        <span class="light-particle particle-one"></span>
        <span class="light-particle particle-two"></span>
        <span class="light-particle particle-three"></span>
      </div>
      <div class="sculpture-shadow"></div>
    </div>

    <div class="showcase-copy">
      <h2 id="showcase-title">Oriel<span class="wordmark-period">.</span></h2>
      <div class="showcase-caption">
        <p>一扇窗，连接你的工具与灵感。</p>
        <span class="caption-line" aria-hidden="true"></span>
        <span class="caption-end">自在，由此展开</span>
      </div>
    </div>
  </section>
</template>

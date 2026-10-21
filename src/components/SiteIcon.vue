<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const props = defineProps<{ title: string; image?: number[] }>()
const failed = ref(false)
const source = computed(() => props.image?.length
  ? `data:image/png;base64,${btoa(props.image.map(byte => String.fromCharCode(byte)).join(''))}`
  : '')
watch(source, () => { failed.value = false })
</script>

<template>
  <span class="site-icon" aria-hidden="true">
    <img v-if="source && !failed" :src="source" alt="" draggable="false" @error="failed = true" />
    <span v-else>{{ Array.from(title)[0] }}</span>
  </span>
</template>

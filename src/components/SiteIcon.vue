<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { getCachedFavicon, requestFavicon } from '../lib/favicons'

const props = defineProps<{ title: string; entryId?: string; registeredUrl?: string | null }>()
const failed = ref(false)

const cachedSource = computed(() => (props.entryId && props.registeredUrl ? getCachedFavicon(props.entryId, props.registeredUrl) : undefined))
const source = computed(() => cachedSource.value || '')

watch(source, () => { failed.value = false })

watch(() => [props.entryId, props.registeredUrl] as const, ([id, url]) => {
  if (id && url && !getCachedFavicon(id, url)) void requestFavicon(id, url)
}, { immediate: true })
</script>

<template>
  <span class="site-icon" aria-hidden="true">
    <img v-if="source && !failed" :src="source" alt="" draggable="false" @error="failed = true" />
    <span v-else>{{ Array.from(title)[0] }}</span>
  </span>
</template>

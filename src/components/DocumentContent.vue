<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{ text: string; hideTitle?: boolean }>()
// A small text-only Markdown subset. Plugin documentation never executes HTML or scripts.
const blocks = computed(() => {
  const result: { kind: 'heading' | 'paragraph' | 'list' | 'code'; text: string; level?: number; items?: string[] }[] = []
  let code: string[] | null = null
  for (const line of props.text.replaceAll('\r\n', '\n').split('\n')) {
    if (line.startsWith('```')) {
      if (code) { result.push({ kind: 'code', text: code.join('\n') }); code = null }
      else code = []
    } else if (code) code.push(line)
    else if (!line.trim()) result.push({ kind: 'paragraph', text: '' })
    else {
      const heading = /^(#{1,6})\s+(.+)$/.exec(line)
      const item = /^[-*]\s+(.+)$/.exec(line)
      const previous = result.at(-1)
      if (heading) result.push({ kind: 'heading', text: heading[2]!, level: Math.min(heading[1]!.length + 1, 6) })
      else if (item) {
        if (previous?.kind === 'list') previous.items!.push(item[1]!)
        else result.push({ kind: 'list', text: '', items: [item[1]!] })
      } else if (previous?.kind === 'paragraph' && previous.text) previous.text += '\n' + line
      else result.push({ kind: 'paragraph', text: line })
    }
  }
  if (code) result.push({ kind: 'code', text: code.join('\n') })
  return result.filter(block => block.text || block.kind === 'list').filter((block, index) => !(props.hideTitle && index === 0 && block.kind === 'heading' && block.level === 2))
})
</script>

<template>
  <article class="document-content">
    <template v-for="(block, index) in blocks" :key="index">
      <component :is="`h${block.level}`" v-if="block.kind === 'heading'">{{ block.text }}</component>
      <ul v-else-if="block.kind === 'list'"><li v-for="(item, i) in block.items" :key="i">{{ item }}</li></ul>
      <pre v-else-if="block.kind === 'code'"><code>{{ block.text }}</code></pre>
      <p v-else>{{ block.text }}</p>
    </template>
  </article>
</template>

<style scoped>
.document-content { line-height: 1.8; overflow-wrap: anywhere; }
.document-content :is(h2,h3,h4,h5,h6) { margin: 24px 0 8px; font-size: 15px; }
.document-content h2 { font-size: 18px; }
.document-content > :first-child { margin-top: 0; }
.document-content p { margin: 10px 0; white-space: pre-line; }
.document-content ul { padding-left: 22px; }
.document-content pre { padding: 12px; border: 1px solid currentColor; border-radius: 6px; white-space: pre-wrap; font-size: 12px; }
</style>

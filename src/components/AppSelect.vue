<script setup lang="ts" generic="T extends string | number | boolean">
import { computed, nextTick, onBeforeUnmount, ref } from 'vue'
import type { SelectOption } from './select'

const props = defineProps<{
  modelValue: T
  options: SelectOption<T>[]
  ariaLabel?: string
}>()

const emit = defineEmits<{
  (event: 'update:modelValue', value: T): void
  (event: 'change'): void
}>()

const root = ref<HTMLElement | null>(null)
const menu = ref<HTMLElement | null>(null)
const open = ref(false)
const focusedIndex = ref(0)
const menuStyle = ref<Record<string, string>>({})

const selected = computed(() => props.options.find(option => option.value === props.modelValue))
const enabledIndexes = computed(() => props.options
  .map((option, index) => option.disabled ? -1 : index)
  .filter(index => index >= 0))

async function show() {
  if (open.value || !enabledIndexes.value.length) return
  open.value = true
  const selectedIndex = props.options.findIndex(option => option.value === props.modelValue)
  focusedIndex.value = enabledIndexes.value.includes(selectedIndex)
    ? selectedIndex
    : enabledIndexes.value[0]
  window.addEventListener('pointerdown', onOutsidePointer, true)
  window.addEventListener('scroll', placeMenu, true)
  window.addEventListener('resize', placeMenu)
  await nextTick()
  root.value?.focus()
  placeMenu()
}

function close() {
  if (!open.value) return
  open.value = false
  window.removeEventListener('pointerdown', onOutsidePointer, true)
  window.removeEventListener('scroll', placeMenu, true)
  window.removeEventListener('resize', placeMenu)
}

function toggle() {
  open.value ? close() : void show()
}

function choose(option: SelectOption<T>) {
  if (option.disabled) return
  emit('update:modelValue', option.value)
  emit('change')
  close()
  root.value?.focus()
}

function onOutsidePointer(event: PointerEvent) {
  if (root.value && !root.value.contains(event.target as Node)) close()
}

function moveFocus(delta: number) {
  const indexes = enabledIndexes.value
  if (!indexes.length) return
  const current = indexes.indexOf(focusedIndex.value)
  const next = (current + delta + indexes.length) % indexes.length
  focusedIndex.value = indexes[next]
}

function onKeydown(event: KeyboardEvent) {
  if (!open.value) {
    if (event.key === 'Enter' || event.key === ' ' || event.key === 'ArrowDown') {
      event.preventDefault()
      void show()
    }
    return
  }
  if (event.key === 'Escape' || event.key === 'Tab') {
    close()
  } else if (event.key === 'ArrowDown') {
    event.preventDefault()
    moveFocus(1)
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    moveFocus(-1)
  } else if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    const option = props.options[focusedIndex.value]
    if (option) choose(option)
  }
}

function placeMenu() {
  const rootEl = root.value
  if (!rootEl || !menu.value) return
  const rect = rootEl.getBoundingClientRect()
  const edge = 8
  const below = window.innerHeight - rect.bottom - edge
  const above = rect.top - edge
  const openUp = below < 180 && above > below
  const available = openUp ? above : below

  menuStyle.value = {
    maxHeight: `${Math.max(80, Math.min(240, available))}px`,
    top: openUp ? 'auto' : 'calc(100% + 1px)',
    bottom: openUp ? 'calc(100% + 1px)' : 'auto',
  }
}

onBeforeUnmount(close)
</script>

<template>
  <div
    ref="root"
    class="app-select"
    :class="{ open }"
    role="combobox"
    tabindex="0"
    :aria-label="ariaLabel"
    :aria-expanded="open"
    @click="toggle"
    @keydown="onKeydown"
  >
    <span class="app-select-value">
      <span v-if="selected?.colors" class="app-select-swatches" aria-hidden="true">
        <i :style="{ background: `#${selected.colors[0]}` }"></i>
        <i :style="{ background: `#${selected.colors[1]}` }"></i>
      </span>
      <span>{{ selected?.label ?? '' }}</span>
    </span>
    <span class="app-select-caret" aria-hidden="true"></span>
    <div v-if="open" ref="menu" class="app-select-menu" :style="menuStyle" role="listbox">
      <button
        v-for="(option, index) in options"
        :key="String(option.value)"
        type="button"
        class="app-select-option"
        :class="{ selected: option.value === modelValue, focused: index === focusedIndex }"
        :disabled="option.disabled"
        role="option"
        :aria-selected="option.value === modelValue"
        @mousedown.prevent
        @click.stop="choose(option)"
      >
        <span v-if="option.colors" class="app-select-swatches" aria-hidden="true">
          <i :style="{ background: `#${option.colors[0]}` }"></i>
          <i :style="{ background: `#${option.colors[1]}` }"></i>
        </span>
        <span>{{ option.label }}</span>
      </button>
    </div>
  </div>
</template>

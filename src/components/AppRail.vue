<script setup lang="ts">
import { computed } from "vue";
import { GitBranch, Settings, Ship } from "lucide-vue-next";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/app";

export type View = "repos" | "settings";

const props = defineProps<{ view: View }>();
const emit = defineEmits<{ navigate: [view: View] }>();

const store = useAppStore();

const NAV_ITEMS: { view: View; label: string; icon: typeof GitBranch }[] = [
  { view: "repos", label: "仓库", icon: GitBranch },
  { view: "settings", label: "设置", icon: Settings },
];

const accountAlert = computed(() =>
  [store.accounts.github, store.accounts.gitee].some((a) => a.status === "invalid"),
);

function itemClass(view: View) {
  return cn(
    "flex h-8 w-full items-center gap-2 rounded-md px-2 text-[13px] transition-colors",
    props.view === view
      ? "bg-secondary font-medium text-foreground"
      : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
  );
}
</script>

<template>
  <nav data-tauri-drag-region class="flex w-48 shrink-0 flex-col gap-1 border-r bg-card px-2 pb-3 pt-9">
    <div class="mb-2 flex items-center gap-2 px-0.5">
      <div class="flex size-[30px] shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground">
        <Ship class="size-4" />
      </div>
      <span class="text-[13px] font-semibold tracking-tight">GitFerry</span>
    </div>
    <button
      v-for="item in NAV_ITEMS"
      :key="item.view"
      :class="itemClass(item.view)"
      @click="emit('navigate', item.view)"
    >
      <component :is="item.icon" class="size-4" />
      <span class="flex-1 text-left">{{ item.label }}</span>
      <span
        v-if="item.view === 'settings' && accountAlert"
        class="size-[7px] rounded-full bg-destructive"
      />
    </button>
  </nav>
</template>

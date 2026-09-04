<script setup lang="ts">
import { computed } from "vue";
import { GitBranch, Moon, Settings, Ship, Sun, UserRound } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import { useAppStore } from "@/stores/app";
import { useTheme } from "@/composables/useTheme";

const emit = defineEmits<{ settings: [] }>();

const store = useAppStore();
const { theme, toggle } = useTheme();

const accountAlert = computed(() =>
  [store.accounts.github, store.accounts.gitee].some((a) => a.status === "invalid"),
);
</script>

<template>
  <nav class="flex w-12 shrink-0 flex-col items-center gap-1 border-r bg-card py-3">
    <div
      class="mb-2 flex size-8 items-center justify-center rounded-md bg-primary text-primary-foreground"
      title="GitFerry"
    >
      <Ship class="size-4" />
    </div>

    <button
      class="flex size-[34px] items-center justify-center rounded-md bg-secondary text-foreground"
      title="仓库"
    >
      <GitBranch class="size-[17px]" />
    </button>

    <button
      class="relative flex size-[34px] items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
      title="账号"
      @click="emit('settings')"
    >
      <UserRound class="size-[17px]" />
      <span v-if="accountAlert" class="absolute right-1 top-1 size-[7px] rounded-full border-[1.5px] border-card bg-destructive" />
    </button>

    <div class="flex-1" />
    <div class="my-1.5 h-px w-6 bg-border" />

    <button
      class="flex size-[34px] items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
      :title="theme === 'dark' ? '切换到亮色' : '切换到暗色'"
      @click="toggle"
    >
      <Sun v-if="theme === 'dark'" class="size-[17px]" />
      <Moon v-else class="size-[17px]" />
    </button>

    <Button variant="ghost" class="size-[34px] p-0" title="设置" @click="emit('settings')">
      <Settings class="size-[17px]" />
    </Button>
  </nav>
</template>

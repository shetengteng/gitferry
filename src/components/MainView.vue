<script setup lang="ts">
import { computed, ref } from "vue";
import { Loader2 } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useAppStore } from "@/stores/app";
import { cn, formatClock, formatRelativeTime } from "@/lib/utils";
import AppRail from "./AppRail.vue";
import type { View } from "./AppRail.vue";
import ConflictDialog from "./ConflictDialog.vue";
import RepoRow from "./RepoRow.vue";
import SettingsView from "./SettingsView.vue";

const store = useAppStore();
const view = ref<View>("repos");

const conflictPath = ref<string | null>(null);
const conflictOpen = ref(false);
const mirrorError = ref<string | null>(null);

function openConflict(path: string) {
  conflictPath.value = path;
  conflictOpen.value = true;
}

async function onMirror(path: string, value: boolean) {
  mirrorError.value = null;
  try {
    await store.setRepoMirror(path, value);
  } catch (err) {
    mirrorError.value = `镜像模式保存失败：${String(err)}。请检查仓库配置后重试`;
  }
}

const syncSummary = computed(() => {
  if (Object.keys(store.syncStates).length === 0) return null;
  if (store.conflictCount === 0 && store.errorCount === 0) return "全部正常";
  const parts: string[] = [];
  if (store.conflictCount > 0) parts.push(`冲突 ${store.conflictCount}`);
  if (store.errorCount > 0) parts.push(`错误 ${store.errorCount}`);
  return parts.join(" · ");
});

const summaryHasIssue = computed(
  () => store.conflictCount > 0 || store.errorCount > 0,
);

/** 页头副行：N 个启用 · 同步中…/上次同步 X（全仓库最近一次同步成功时间） */
const pageSub = computed(() => {
  if (store.syncingAll) return `${store.enabledCount} 个启用 · 同步中…`;
  const rel = formatRelativeTime(store.lastSyncedAt);
  return rel
    ? `${store.enabledCount} 个启用 · 上次同步 ${rel}`
    : `${store.enabledCount} 个启用 · 尚未同步`;
});

/** 底部状态栏左侧：N 个启用 · 上次全量同步 HH:MM */
const statusbarLeft = computed(() => {
  if (store.lastSyncAllAt === null) return `${store.enabledCount} 个启用`;
  return `${store.enabledCount} 个启用 · 上次全量同步 ${formatClock(store.lastSyncAllAt)}`;
});

const STAT_CARDS = [
  { key: "synced", label: "已同步", dot: "bg-success" },
  { key: "pending", label: "待同步", dot: "bg-warning" },
  { key: "conflict", label: "冲突", dot: "bg-destructive" },
  { key: "error", label: "错误", dot: "bg-destructive" },
] as const;

async function rescan() {
  await store.scan();
}
</script>

<template>
  <div class="flex h-screen">
    <AppRail :view="view" @navigate="view = $event" />

    <template v-if="view === 'repos'">
      <div class="flex min-w-0 flex-1 flex-col">
        <div data-tauri-drag-region class="flex flex-col gap-2.5 px-4 pb-3.5 pt-4">
          <div class="flex items-center gap-2">
            <div class="min-w-0">
              <div class="text-[17px] font-semibold leading-tight tracking-tight">仓库</div>
              <div class="mt-0.5 text-xs text-muted-foreground">{{ pageSub }}</div>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <Input v-model="store.query" class="h-8 flex-1 text-xs" placeholder="搜索仓库名或路径…" />
            <Button variant="outline" size="sm" :disabled="store.scanning" @click="rescan">
              {{ store.scanning ? "扫描中…" : "重新扫描" }}
            </Button>
            <Button
              size="sm"
              :disabled="store.syncingAll || store.enabledCount === 0"
              :title="store.enabledCount === 0 ? '没有已启用的仓库' : undefined"
              @click="store.syncAll"
            >
              <Loader2 v-if="store.syncingAll" class="mr-1 h-3 w-3 animate-spin" />
              {{ store.syncingAll ? "同步中…" : "立即全量同步" }}
            </Button>
          </div>
        </div>

        <div class="flex gap-2 px-4 pb-3.5">
          <div
            v-for="card in STAT_CARDS"
            :key="card.key"
            class="flex flex-1 items-center gap-2 rounded-lg border bg-card px-3 py-2"
            :class="cn(store.stats[card.key] === 0 && 'opacity-55')"
          >
            <span class="size-1.5 shrink-0 rounded-full" :class="card.dot" />
            <span class="text-[15px] font-semibold tabular-nums">{{ store.stats[card.key] }}</span>
            <span class="text-[11px] text-muted-foreground">{{ card.label }}</span>
          </div>
        </div>

        <p
          v-if="store.syncingError"
          class="border-y border-destructive/25 bg-destructive/10 px-4 py-1.5 text-xs text-destructive"
        >
          {{ store.syncingError }}
        </p>
        <p
          v-if="mirrorError"
          class="border-y border-destructive/25 bg-destructive/10 px-4 py-1.5 text-xs text-destructive"
        >
          {{ mirrorError }}
        </p>

        <div class="flex flex-1 flex-col gap-2 overflow-y-auto px-4 pb-3.5">
          <p
            v-if="store.visibleRepos.length === 0"
            class="my-auto flex flex-col items-center justify-center gap-2 text-sm text-muted-foreground"
          >
            未发现 git 仓库，请检查扫描目录或添加新目录
          </p>
          <RepoRow
            v-for="repo in store.visibleRepos"
            :key="repo.path"
            :repo="repo"
            :sync-state="store.syncStateOf(repo.path)"
            @change="store.setRepo"
            @sync="store.syncRepo"
            @conflict="openConflict"
            @mirror="onMirror"
          />
        </div>

        <div class="flex items-center justify-between border-t px-4 py-2 text-[11px] text-muted-foreground">
          <span>{{ statusbarLeft }}</span>
          <span :class="cn(summaryHasIssue && 'font-medium text-destructive')">
            {{ syncSummary ?? "尚未同步" }}
          </span>
        </div>
      </div>

      <ConflictDialog
        v-if="conflictPath"
        v-model:open="conflictOpen"
        :path="conflictPath"
      />
    </template>

    <SettingsView v-else />
  </div>
</template>

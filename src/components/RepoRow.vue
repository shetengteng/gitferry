<script setup lang="ts">
import { computed } from "vue";
import { RefreshCw } from "lucide-vue-next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { askConfirm } from "@/lib/dialog";
import StatusBadge from "./StatusBadge.vue";
import type { Direction, RepoEntry, RepoSyncState, SyncStatus } from "@/lib/types";
import { cn, formatRelativeTime } from "@/lib/utils";

const props = defineProps<{ repo: RepoEntry; syncState?: RepoSyncState }>();

const emit = defineEmits<{
  change: [path: string, enabled: boolean, direction: Direction];
  sync: [path: string];
  conflict: [path: string];
  mirror: [path: string, value: boolean];
}>();

const KIND_LABEL: Record<RepoEntry["kind"], string> = {
  both: "双端",
  github_only: "GitHub",
  gitee_only: "Gitee",
  none: "无 remote",
};

const DIRECTIONS: { value: Direction; label: string }[] = [
  { value: "both", label: "双向" },
  { value: "github_to_gitee", label: "GitHub → Gitee" },
  { value: "gitee_to_github", label: "Gitee → GitHub" },
];

const SYNC_META: Record<SyncStatus, { tone: "success" | "warning" | "info" | "destructive" | "secondary"; label: string }> = {
  synced: { tone: "success", label: "已同步" },
  pending: { tone: "secondary", label: "待同步" },
  syncing: { tone: "info", label: "同步中…" },
  conflict: { tone: "destructive", label: "冲突" },
  error: { tone: "destructive", label: "错误" },
  missing: { tone: "warning", label: "路径失效" },
};

const repoName = computed(() => props.repo.path.split("/").filter(Boolean).pop() ?? props.repo.path);

const status = computed(() => props.syncState?.status ?? null);

const statusMeta = computed(() => (status.value ? SYNC_META[status.value] : null));

const conflictLabel = computed(() => {
  const n = props.syncState?.conflicts.length ?? 0;
  return n > 0 ? `冲突 · ${n}` : "冲突";
});

const statusTitle = computed(() => {
  if (status.value === "error" && props.syncState?.error) return props.syncState.error;
  if (status.value === "missing") return "仓库路径无法访问，请检查是否被移动或删除";
  if (status.value === "conflict") return "分支分叉，点击处理";
  return undefined;
});

const syncedAt = computed(() => {
  if (status.value !== "synced") return null;
  return formatRelativeTime(props.syncState?.last_synced ?? null);
});

/** 第二行右侧 meta：synced→相对时间；conflict→暂停同步；error→见错误详情；其余→— */
const metaText = computed(() => {
  switch (status.value) {
    case "synced":
      return syncedAt.value ?? "—";
    case "conflict":
      return "暂停同步";
    case "error":
      return "见错误详情";
    default:
      return "—";
  }
});

const metaTitle = computed(() =>
  status.value === "error" ? (props.syncState?.error ?? undefined) : undefined,
);

const rowClass = computed(() =>
  cn(
    "flex flex-col gap-2 rounded-lg border bg-card px-4 py-3 transition-colors hover:border-muted-foreground/30 hover:shadow-sm",
    status.value === "conflict" &&
      "bg-warning/5 shadow-[inset_2px_0_0_hsl(var(--warning))] hover:shadow-[inset_2px_0_0_hsl(var(--warning))]",
    !props.repo.enabled && "opacity-45",
  ),
);

const syncDisabled = computed(
  () =>
    !props.repo.enabled ||
    status.value === "syncing" ||
    props.repo.kind === "none",
);

function onEnabled(enabled: boolean) {
  emit("change", props.repo.path, enabled, props.repo.direction);
}

function onDirection(value: unknown) {
  if (value === "both" || value === "github_to_gitee" || value === "gitee_to_github") {
    emit("change", props.repo.path, props.repo.enabled, value);
  }
}

/** 开启镜像删除需用户显式确认；取消则不 emit（model-value 单向绑定自动回弹） */
async function toggleMirror(next: boolean) {
  if (next) {
    const ok = await askConfirm(
      "开启镜像删除？",
      "开启后自动同步将沿同步方向删除远端已不存在的分支与标签（双向时两端对称清理）。远端删除将同步生效，不可自动撤销。",
    );
    if (!ok) return;
  }
  emit("mirror", props.repo.path, next);
}
</script>

<template>
  <div :class="rowClass">
    <div class="flex items-center gap-2">
      <Switch :model-value="repo.enabled" @update:model-value="onEnabled" />
      <span class="text-[13px] font-semibold tracking-tight">{{ repoName }}</span>
      <Badge variant="secondary">{{ KIND_LABEL[repo.kind] }}</Badge>
      <Badge v-for="r in repo.remotes" :key="r.name + r.url" variant="outline" class="max-w-40 truncate font-mono text-[10px] font-normal">
        {{ r.name }}
      </Badge>
      <div v-if="statusMeta" class="ml-auto flex shrink-0 items-center gap-1.5" :title="statusTitle">
        <button
          v-if="status === 'conflict'"
          type="button"
          class="rounded-full outline-none transition-opacity hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring"
          @click="emit('conflict', repo.path)"
        >
          <StatusBadge tone="destructive">{{ conflictLabel }}</StatusBadge>
        </button>
        <StatusBadge v-else :tone="statusMeta.tone">{{ statusMeta.label }}</StatusBadge>
      </div>
      <Button
        variant="ghost"
        size="icon"
        class="h-7 w-7 shrink-0 text-muted-foreground hover:text-foreground"
        :disabled="syncDisabled"
        :title="syncDisabled ? '仓库未启用或正在同步' : '立即同步'"
        @click="emit('sync', repo.path)"
      >
        <RefreshCw class="h-3.5 w-3.5" :class="cn(status === 'syncing' && 'animate-spin')" />
      </Button>
    </div>
    <div class="flex items-center gap-2 pl-2">
      <span class="flex-1 truncate font-mono text-[11px] text-muted-foreground">{{ repo.path }}</span>
      <Select
        v-if="repo.kind !== 'none'"
        :model-value="repo.direction"
        @update:model-value="onDirection"
      >
        <SelectTrigger class="h-7 w-40 text-xs">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="d in DIRECTIONS" :key="d.value" :value="d.value">{{ d.label }}</SelectItem>
        </SelectContent>
      </Select>
      <Input v-else class="h-7 w-40 text-[11px]" placeholder="先添加 remote（git remote add）" disabled />
      <div
        v-if="repo.enabled && repo.kind !== 'none'"
        class="flex shrink-0 items-center gap-1.5"
      >
        <span class="whitespace-nowrap text-xs text-muted-foreground">镜像删除</span>
        <Switch
          :model-value="repo.mirror_delete"
          @update:model-value="toggleMirror"
        />
      </div>
      <span
        class="shrink-0 whitespace-nowrap text-[11px] text-muted-foreground"
        :title="metaTitle"
      >
        {{ metaText }}
      </span>
    </div>
  </div>
</template>

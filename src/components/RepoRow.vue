<script setup lang="ts">
import { computed } from "vue";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { Direction, RepoEntry } from "@/lib/types";

const props = defineProps<{ repo: RepoEntry }>();

const emit = defineEmits<{
  change: [path: string, enabled: boolean, direction: Direction];
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

const repoName = computed(() => props.repo.path.split("/").filter(Boolean).pop() ?? props.repo.path);

function onEnabled(enabled: boolean) {
  emit("change", props.repo.path, enabled, props.repo.direction);
}

function onDirection(value: unknown) {
  if (value === "both" || value === "github_to_gitee" || value === "gitee_to_github") {
    emit("change", props.repo.path, props.repo.enabled, value);
  }
}
</script>

<template>
  <div
    class="flex flex-col gap-2 border-b px-4 py-3 transition-colors hover:bg-accent/50"
    :class="{ 'opacity-45': !repo.enabled }"
  >
    <div class="flex items-center gap-2">
      <Switch :model-value="repo.enabled" @update:model-value="onEnabled" />
      <span class="text-[13px] font-semibold tracking-tight">{{ repoName }}</span>
      <Badge variant="secondary">{{ KIND_LABEL[repo.kind] }}</Badge>
      <Badge v-for="r in repo.remotes" :key="r.name + r.url" variant="outline" class="max-w-40 truncate font-mono text-[10px] font-normal">
        {{ r.name }}
      </Badge>
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
    </div>
  </div>
</template>

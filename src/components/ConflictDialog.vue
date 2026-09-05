<script setup lang="ts">
import { computed, ref } from "vue";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useAppStore } from "@/stores/app";
import type { ConflictSide } from "@/lib/types";

const props = defineProps<{ path: string }>();
const open = defineModel<boolean>("open", { required: true });

const store = useAppStore();
const resolving = ref<ConflictSide | null>(null);
const error = ref<string | null>(null);

const conflicts = computed(() => store.syncStateOf(props.path)?.conflicts ?? []);

const KIND_LABEL = { branch: "分支", tag: "标签" } as const;

async function resolve(side: ConflictSide) {
  resolving.value = side;
  error.value = null;
  try {
    await store.resolveConflict(props.path, side);
    open.value = false;
  } catch (err) {
    error.value = String(err);
  } finally {
    resolving.value = null;
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-lg">
      <DialogHeader>
        <DialogTitle>检测到分支分叉</DialogTitle>
        <DialogDescription>
          两端仓库各有新提交，自动同步已暂停。选择权威一侧后将以强制推送对齐（该操作会覆盖另一侧的提交，无法恢复）；也可以先在本地手动合并后重新同步。
        </DialogDescription>
      </DialogHeader>

      <div class="space-y-2">
        <template v-if="conflicts.length > 0">
          <div class="flex items-center gap-2 px-3 text-[10px] text-muted-foreground">
            <span class="flex-1">引用</span>
            <span class="w-20 text-right">GitHub</span>
            <span class="w-20 text-right">Gitee</span>
          </div>
          <div
            v-for="c in conflicts"
            :key="c.kind + ':' + c.name"
            class="flex items-center gap-2 rounded-md border px-3 py-2 text-xs"
          >
            <Badge variant="outline" class="shrink-0 font-normal">{{ KIND_LABEL[c.kind] }}</Badge>
            <span class="flex-1 truncate font-mono">{{ c.name }}</span>
            <span class="w-20 shrink-0 text-right font-mono text-[11px] text-muted-foreground">{{ c.github_sha }}</span>
            <span class="w-20 shrink-0 text-right font-mono text-[11px] text-muted-foreground">{{ c.gitee_sha }}</span>
          </div>
        </template>
        <p v-else class="text-xs text-muted-foreground">
          未找到冲突详情，可尝试重新同步后再次查看。
        </p>

        <p v-if="error" class="text-xs text-destructive">{{ error }}</p>
      </div>

      <DialogFooter class="gap-2 sm:gap-0">
        <Button variant="outline" :disabled="resolving !== null" @click="open = false">
          稍后处理
        </Button>
        <Button
          variant="secondary"
          :disabled="resolving !== null"
          @click="resolve('gitee')"
        >
          {{ resolving === "gitee" ? "对齐中…" : "以 Gitee 为准" }}
        </Button>
        <Button
          variant="destructive"
          :disabled="resolving !== null"
          @click="resolve('github')"
        >
          {{ resolving === "github" ? "对齐中…" : "以 GitHub 为准" }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

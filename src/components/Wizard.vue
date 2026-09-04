<script setup lang="ts">
import { computed, ref } from "vue";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useAppStore } from "@/stores/app";

const store = useAppStore();
const step = ref<1 | 2>(1);
const selected = ref<Set<string>>(new Set());

const KIND_LABEL: Record<string, string> = {
  both: "github + gitee",
  github_only: "github",
  gitee_only: "gitee",
  none: "无 remote",
};

const found = computed(() => store.repos);
const allSelected = computed(
  () => found.value.length > 0 && found.value.every((r) => selected.value.has(r.path)),
);

function toggle(path: string) {
  const next = new Set(selected.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  selected.value = next;
}

function toggleAll() {
  if (allSelected.value) {
    selected.value = new Set();
  } else {
    selected.value = new Set(found.value.map((r) => r.path));
  }
}

async function startScan() {
  await store.scan();
  if (found.value.length > 0) {
    selected.value = new Set(
      found.value.filter((r) => r.kind !== "none").map((r) => r.path),
    );
    step.value = 2;
  } else {
    // 没发现仓库也允许完成，进入主界面再调整扫描目录
    await store.completeSetup();
  }
}

async function finish() {
  for (const repo of found.value) {
    if (selected.value.has(repo.path) !== repo.enabled) {
      await store.setRepo(repo.path, selected.value.has(repo.path), repo.direction);
    }
  }
  await store.completeSetup();
}
</script>

<template>
  <div class="mx-auto flex h-screen max-w-lg flex-col px-6 py-10">
    <div class="mb-6">
      <h1 class="text-lg font-semibold tracking-tight">
        {{ step === 1 ? "让 GitHub 与 Gitee 之间的同步自动发生" : "扫描结果确认" }}
      </h1>
      <p v-if="step === 1" class="mt-2 text-xs leading-relaxed text-muted-foreground">
        GitFerry 会扫描下方目录中的 git 仓库，识别 remote 归属（GitHub / Gitee），
        按你为每个仓库设置的开关与方向，在后台定时自动同步。同步复用系统 git 与钥匙串凭据，不需要配置 webhook 或令牌。
      </p>
    </div>

    <div v-if="step === 1" class="flex-1 space-y-2">
      <h3 class="text-xs font-semibold">扫描目录</h3>
      <div
        v-for="root in store.settings.scan_roots"
        :key="root"
        class="flex items-center gap-2 rounded-md border px-3 py-2 font-mono text-xs"
      >
        {{ root }}
      </div>
      <p v-if="store.settings.scan_roots.length === 0" class="text-xs text-muted-foreground">
        尚无扫描目录，请进入设置添加。
      </p>
      <p class="text-[11px] text-muted-foreground">
        默认深度 {{ store.settings.max_depth }} 层，自动跳过
        <code class="font-mono">node_modules</code>、隐藏目录与 <code class="font-mono">.git</code> 内部。
      </p>
    </div>

    <div v-else class="flex-1 space-y-3 overflow-y-auto">
      <div class="rounded-md border p-3">
        <div class="flex items-center justify-between text-xs">
          <span>扫描完成 · 发现 <b>{{ found.length }}</b> 个 git 仓库</span>
        </div>
      </div>
      <div class="flex items-center justify-between">
        <h3 class="text-xs font-semibold">选择要启用的仓库</h3>
        <Button variant="ghost" size="sm" @click="toggleAll">
          {{ allSelected ? "全不选" : "全选" }}
        </Button>
      </div>
      <div
        v-for="repo in found"
        :key="repo.path"
        class="flex items-center gap-3 rounded-md border px-3 py-2 text-xs"
      >
        <input
          type="checkbox"
          class="size-3.5 accent-[hsl(var(--primary))]"
          :checked="selected.has(repo.path)"
          @change="toggle(repo.path)"
        />
        <span class="flex-1 truncate font-medium">{{ repo.path.split("/").filter(Boolean).pop() }}</span>
        <span class="font-mono text-[11px] text-muted-foreground">{{ KIND_LABEL[repo.kind] }}</span>
      </div>
      <p class="text-[11px] leading-relaxed text-muted-foreground">
        无 remote 的仓库也可以启用——同步方向若指向不存在的目标仓库，会提示先创建。
        可稍后在主界面逐个开启。
      </p>
    </div>

    <div class="mt-6 flex items-center justify-between gap-2">
      <Badge v-if="store.scanError" variant="destructive">{{ store.scanError }}</Badge>
      <span v-else class="flex-1" />
      <Button variant="ghost" @click="store.completeSetup()">稍后再说</Button>
      <Button :disabled="store.scanning" @click="step === 1 ? startScan() : finish()">
        {{ store.scanning ? "扫描中…" : step === 1 ? "开始扫描" : "启用所选并完成" }}
      </Button>
    </div>
  </div>
</template>

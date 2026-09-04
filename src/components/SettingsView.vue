<script setup lang="ts">
import { computed, ref } from "vue";
import { Button } from "@/components/ui/button";
import { useAppStore } from "@/stores/app";
import AccountDialog from "./AccountDialog.vue";
import StatusBadge from "./StatusBadge.vue";
import type { AccountStatus, Platform } from "@/lib/types";

const store = useAppStore();
const maxDepth = ref(String(store.settings.max_depth));
const accountDialog = ref<Platform | null>(null);
const accountDialogVisible = computed({
  get: () => accountDialog.value !== null,
  set: (value) => {
    if (!value) accountDialog.value = null;
  },
});

const ACCOUNT_STATUS: Record<AccountStatus, { tone: "success" | "warning" | "secondary"; label: string }> = {
  unconfigured: { tone: "secondary", label: "未配置" },
  connected: { tone: "success", label: "已连接" },
  invalid: { tone: "warning", label: "令牌失效" },
};

async function addRoot() {
  const { open: pick } = await import("@tauri-apps/plugin-dialog");
  const selected = await pick({ directory: true, multiple: false });
  if (typeof selected === "string") {
    await store.addScanRoot(selected);
  }
}
</script>

<template>
  <div class="flex-1 overflow-y-auto">
    <div class="mx-auto w-full max-w-2xl space-y-6 px-6 py-6">
      <div class="space-y-1">
        <h1 class="text-lg font-semibold tracking-tight">设置</h1>
        <p class="text-sm text-muted-foreground">账号、扫描目录与轮询参数。改动即时生效并保存。</p>
      </div>

      <section class="space-y-2">
        <h3 class="text-xs font-semibold">账号</h3>
        <div class="flex items-center justify-between rounded-md border px-3 py-2">
          <span class="flex items-center gap-2 text-sm font-medium">
            GitHub
            <StatusBadge :tone="ACCOUNT_STATUS[store.accounts.github.status].tone">
              {{ ACCOUNT_STATUS[store.accounts.github.status].label
              }}{{ store.accounts.github.login ? ` @${store.accounts.github.login}` : "" }}
            </StatusBadge>
          </span>
          <Button variant="outline" size="sm" @click="accountDialog = 'github'">
            {{ store.accounts.github.status === "unconfigured" ? "配置令牌" : "更新令牌" }}
          </Button>
        </div>
        <div class="flex items-center justify-between rounded-md border px-3 py-2">
          <span class="flex items-center gap-2 text-sm font-medium">
            Gitee
            <StatusBadge :tone="ACCOUNT_STATUS[store.accounts.gitee.status].tone">
              {{ ACCOUNT_STATUS[store.accounts.gitee.status].label
              }}{{ store.accounts.gitee.login ? ` @${store.accounts.gitee.login}` : "" }}
            </StatusBadge>
          </span>
          <Button variant="outline" size="sm" @click="accountDialog = 'gitee'">
            {{ store.accounts.gitee.status === "unconfigured" ? "配置令牌" : "更新令牌" }}
          </Button>
        </div>
        <p class="text-[11px] leading-relaxed text-muted-foreground">
          令牌保存在 macOS 钥匙串，GitFerry 不落盘明文。GitHub 需
          <code class="font-mono">repo</code> 权限，Gitee 需
          <code class="font-mono">projects</code> 权限，用于账号检测与目标仓库自动创建。
        </p>
      </section>

      <section class="space-y-2">
        <h3 class="text-xs font-semibold">扫描目录</h3>
        <div
          v-for="root in store.settings.scan_roots"
          :key="root"
          class="flex items-center gap-2 rounded-md border px-3 py-2"
        >
          <span class="flex-1 truncate font-mono text-xs">{{ root }}</span>
          <Button
            variant="ghost"
            size="sm"
            class="h-6 text-muted-foreground hover:text-destructive"
            @click="store.removeScanRoot(root)"
          >
            ×
          </Button>
        </div>
        <Button variant="outline" size="sm" @click="addRoot">＋ 添加目录…</Button>
        <p class="text-[11px] text-muted-foreground">
          默认深度内自动跳过 <code class="font-mono">node_modules</code>、隐藏目录与
          <code class="font-mono">.git</code> 内部。
        </p>
      </section>

      <section class="flex items-center justify-between">
        <span class="text-sm">扫描深度</span>
        <select
          v-model="maxDepth"
          class="h-8 rounded-md border border-input bg-background px-2 text-xs"
          @change="store.setMaxDepth(Number(maxDepth))"
        >
          <option value="2">2 层</option>
          <option value="3">3 层</option>
          <option value="4">4 层</option>
          <option value="5">5 层</option>
        </select>
      </section>
    </div>
  </div>

  <AccountDialog v-if="accountDialog" v-model:open="accountDialogVisible" :platform="accountDialog" />
</template>

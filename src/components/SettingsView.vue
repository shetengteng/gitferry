<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { pickDirectory } from "@/lib/dialog";
import { useAppStore } from "@/stores/app";
import { useTheme } from "@/composables/useTheme";
import AccountDialog from "./AccountDialog.vue";
import StatusBadge from "./StatusBadge.vue";
import type { AccountStatus, Platform } from "@/lib/types";

const store = useAppStore();
const { theme, set: setTheme } = useTheme();
const maxDepth = ref(String(store.settings.max_depth));
const syncInterval = ref(String(store.settings.sync_interval_mins));
const concurrency = ref(String(store.settings.concurrency));
const accountDialog = ref<Platform | null>(null);
const accountDialogVisible = computed({
  get: () => accountDialog.value !== null,
  set: (value) => {
    if (!value) accountDialog.value = null;
  },
});
const revealing = ref(false);
const revealError = ref<string | null>(null);
const rootError = ref<string | null>(null);

const ACCOUNT_STATUS: Record<AccountStatus, { tone: "success" | "warning" | "secondary"; label: string }> = {
  unconfigured: { tone: "secondary", label: "未配置" },
  connected: { tone: "success", label: "已连接" },
  invalid: { tone: "warning", label: "令牌失效" },
};

async function addRoot() {
  rootError.value = null;
  try {
    const root = await pickDirectory();
    if (root) {
      await store.addScanRoot(root);
    }
  } catch (err) {
    rootError.value = String(err);
  }
}

async function removeRoot(root: string) {
  rootError.value = null;
  try {
    await store.removeScanRoot(root);
  } catch (err) {
    rootError.value = String(err);
  }
}

async function revealLogs() {
  revealing.value = true;
  revealError.value = null;
  try {
    await store.revealLogs();
  } catch (err) {
    revealError.value = String(err);
  } finally {
    revealing.value = false;
  }
}

onMounted(() => {
  store.refreshAutostart();
});

async function toggleAutostart(checked: boolean) {
  await store.toggleAutostart(checked);
}
</script>

<template>
  <div class="flex min-w-0 flex-1 flex-col">
    <div class="flex-1 overflow-y-auto">
      <div data-tauri-drag-region class="mx-auto w-full max-w-2xl space-y-6 px-6 py-6">
        <div class="space-y-1">
          <h1 class="text-lg font-semibold tracking-tight">设置</h1>
          <p class="text-sm text-muted-foreground">账号、扫描目录与轮询参数。改动即时生效并保存。</p>
        </div>

        <section class="space-y-2">
          <h3 class="text-xs font-semibold">账号</h3>
          <div class="flex items-center justify-between rounded-md border px-3 py-2">
            <span class="flex items-center gap-2 text-xs font-medium">
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
            <span class="flex items-center gap-2 text-xs font-medium">
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
              @click="removeRoot(root)"
            >
              ×
            </Button>
          </div>
          <Button variant="outline" size="sm" @click="addRoot">＋ 添加目录…</Button>
          <p v-if="rootError" class="text-[11px] text-destructive">{{ rootError }}</p>
          <p class="text-[11px] text-muted-foreground">
            默认深度内自动跳过 <code class="font-mono">node_modules</code>、隐藏目录与
            <code class="font-mono">.git</code> 内部。
          </p>
        </section>

        <section class="flex items-center justify-between">
          <span class="text-sm">外观</span>
          <select
            class="h-8 rounded-md border border-input bg-background px-2 text-xs"
            :value="theme"
            @change="setTheme(($event.target as HTMLSelectElement).value as 'light' | 'dark')"
          >
            <option value="light">亮色</option>
            <option value="dark">暗色</option>
          </select>
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

        <section class="flex items-center justify-between">
          <span class="text-sm">自动同步间隔</span>
          <select
            v-model="syncInterval"
            class="h-8 rounded-md border border-input bg-background px-2 text-xs"
            @change="store.setSyncInterval(Number(syncInterval))"
          >
            <option value="5">5 分钟</option>
            <option value="10">10 分钟</option>
            <option value="30">30 分钟</option>
            <option value="60">1 小时</option>
            <option value="360">6 小时</option>
          </select>
        </section>

        <section class="flex items-center justify-between">
          <span class="text-sm">并发同步数</span>
          <select
            v-model="concurrency"
            class="h-8 rounded-md border border-input bg-background px-2 text-xs"
            @change="store.setConcurrency(Number(concurrency))"
          >
            <option value="1">1</option>
            <option value="2">2</option>
            <option value="3">3</option>
            <option value="4">4</option>
          </select>
        </section>

        <section class="flex items-center justify-between">
          <div class="space-y-0.5">
            <span class="text-sm">开机自启</span>
            <p class="text-[11px] text-muted-foreground">
              登录后自动启动 GitFerry。正式版安装后请重新开关一次以更新启动路径
            </p>
          </div>
          <div class="flex items-center gap-2">
            <span v-if="store.autostartError" class="text-[11px] text-destructive">{{ store.autostartError }}</span>
            <Switch
              :model-value="store.autostartEnabled"
              @update:model-value="toggleAutostart"
            />
          </div>
        </section>

        <section class="flex items-center justify-between">
          <span class="text-sm">日志</span>
          <div class="flex items-center gap-2">
            <span v-if="revealError" class="text-[11px] text-destructive">{{ revealError }}</span>
            <Button variant="outline" size="sm" :disabled="revealing" @click="revealLogs">
              {{ revealing ? "打开中…" : "打开日志目录" }}
            </Button>
          </div>
        </section>

        <p class="text-[11px] text-muted-foreground">
          自动同步间隔与并发数由调度器（M3）消费，保存后于下个轮询周期生效。
        </p>
      </div>
    </div>
  </div>

  <AccountDialog v-if="accountDialog" v-model:open="accountDialogVisible" :platform="accountDialog" />
</template>

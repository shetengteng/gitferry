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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useAppStore } from "@/stores/app";
import type { Platform } from "@/lib/types";

const props = defineProps<{ platform: Platform }>();
const open = defineModel<boolean>("open", { required: true });

const store = useAppStore();
const token = ref("");
const saving = ref(false);
const error = ref<string | null>(null);

const LABELS: Record<Platform, { title: string; guide: string; perm: string; api: string }> = {
  github: {
    title: "配置 GitHub 账号",
    guide: "在 GitHub「Settings → Developer settings → Personal access tokens」生成令牌",
    perm: "repo",
    api: "api.github.com/user",
  },
  gitee: {
    title: "配置 Gitee 账号",
    guide: "在 Gitee「设置 → 安全设置 → 私人令牌」生成令牌",
    perm: "projects",
    api: "api/v5/user",
  },
};

const current = computed(() => store.accounts[props.platform]);

async function save() {
  if (!token.value.trim()) {
    error.value = "请粘贴令牌";
    return;
  }
  saving.value = true;
  error.value = null;
  try {
    await store.configureAccount(props.platform, token.value.trim());
    token.value = "";
    open.value = false;
  } catch (err) {
    error.value = String(err);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>{{ LABELS[platform].title }}</DialogTitle>
        <DialogDescription>
          {{ LABELS[platform].guide }}，勾选
          <code class="rounded bg-muted px-1 font-mono text-xs">{{ LABELS[platform].perm }}</code>
          权限。保存后 GitFerry 调用
          <code class="rounded bg-muted px-1 font-mono text-xs">{{ LABELS[platform].api }}</code>
          验证并显示账号。令牌只存 macOS 钥匙串，GitFerry 不落盘明文。
        </DialogDescription>
      </DialogHeader>

      <div class="space-y-2">
        <Label for="token">私人令牌</Label>
        <Input id="token" v-model="token" type="password" class="font-mono" placeholder="粘贴 personal access token" />
        <p v-if="error" class="text-xs text-destructive">{{ error }}</p>
        <div v-else-if="current.status === 'connected'" class="flex items-center gap-2 rounded-md border border-success/25 bg-success/10 px-3 py-2 text-xs">
          <Badge variant="success">验证通过</Badge>
          <span class="ml-auto font-mono">已连接 @{{ current.login }}</span>
        </div>
        <p v-else-if="current.status === 'invalid'" class="text-xs text-warning">当前令牌已失效，请重新配置。</p>
      </div>

      <DialogFooter>
        <Button variant="outline" @click="open = false">取消</Button>
        <Button :disabled="saving" @click="save">{{ saving ? "验证中…" : "保存并验证" }}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

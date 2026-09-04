<script setup lang="ts">
import { ref } from "vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useAppStore } from "@/stores/app";
import AppRail from "./AppRail.vue";
import RepoRow from "./RepoRow.vue";
import SettingsDialog from "./SettingsDialog.vue";

const store = useAppStore();
const settingsOpen = ref(false);

async function rescan() {
  await store.scan();
}
</script>

<template>
  <div class="flex h-screen">
    <AppRail @settings="settingsOpen = true" />
    <div class="flex min-w-0 flex-1 flex-col">
      <div class="flex items-center gap-2 border-b px-4 py-2.5">
        <Input v-model="store.query" class="h-8 flex-1 text-xs" placeholder="搜索仓库名或路径…" />
        <Button variant="outline" size="sm" :disabled="store.scanning" @click="rescan">
          {{ store.scanning ? "扫描中…" : "重新扫描" }}
        </Button>
      </div>

      <div class="flex-1 overflow-y-auto">
        <p
          v-if="store.visibleRepos.length === 0"
          class="flex h-full flex-col items-center justify-center gap-2 text-sm text-muted-foreground"
        >
          未发现 git 仓库，请检查扫描目录或添加新目录
        </p>
        <RepoRow
          v-for="repo in store.visibleRepos"
          :key="repo.path"
          :repo="repo"
          @change="store.setRepo"
        />
      </div>

      <div class="flex items-center justify-between border-t px-4 py-2 text-[11px] text-muted-foreground">
        <span>{{ store.enabledCount }} 个启用</span>
        <span>M2 将支持自动同步</span>
      </div>
    </div>

    <SettingsDialog v-model:open="settingsOpen" />
  </div>
</template>

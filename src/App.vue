<script setup lang="ts">
import { onMounted } from "vue";
import { useAppStore } from "@/stores/app";
import MainView from "@/components/MainView.vue";
import Wizard from "@/components/Wizard.vue";

const store = useAppStore();

onMounted(() => {
  store.init();
});
</script>

<template>
  <div v-if="!store.loaded" class="flex h-screen items-center justify-center text-sm text-muted-foreground">
    加载中…
  </div>
  <Wizard v-else-if="!store.setupDone" />
  <MainView v-else />
</template>

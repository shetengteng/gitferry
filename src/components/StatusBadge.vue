<script setup lang="ts">
import { computed } from "vue";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

const props = defineProps<{ tone: "success" | "warning" | "info" | "destructive" | "secondary" }>();

const TONE_CLASS: Record<string, string> = {
  success: "border-success/25 bg-success/12 text-success",
  warning: "border-warning/25 bg-warning/12 text-warning",
  info: "border-info/25 bg-info/12 text-info",
  destructive: "border-destructive/25 bg-destructive/12 text-destructive",
  secondary: "",
};

const variant = computed(() => (props.tone === "secondary" ? "secondary" : "outline"));
</script>

<template>
  <Badge :variant="variant" :class="cn(TONE_CLASS[tone])"><slot /></Badge>
</template>

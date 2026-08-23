<script setup lang="ts">
import RiscvCompileConfigForm from "./RiscvCompileConfigForm.vue";

defineProps<{ language: string }>();
</script>

<template>
  <RiscvCompileConfigForm v-if="language === 'RISC-V'" />
</template>

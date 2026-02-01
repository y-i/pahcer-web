<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type GlobalConfig, type LocalConfig } from './api';

  let globalConfig = $state<GlobalConfig>({
    visualizerPosition: 'right',
    visualizerUrl: '',
    defaultSeed: 0,
    defaultScale: 1.0
  });

  let localConfig = $state<LocalConfig>({
    visualizerUrl: ''
  });

  let isLoading = $state(true);
  let message = $state('');

  onMount(async () => {
    try {
      const res = await api.getConfig();
      globalConfig = { ...globalConfig, ...res.global };
      localConfig = { ...localConfig, ...res.local };
    } catch (e) {
      console.error('Failed to load config', e);
    } finally {
      isLoading = false;
    }
  });

  async function saveGlobal() {
    try {
      await api.saveGlobalConfig(globalConfig);
      message = 'Global config saved!';
      setTimeout(() => message = '', 3000);
    } catch (e) {
      message = 'Failed to save global config';
    }
  }

  async function saveLocal() {
    try {
      await api.saveLocalConfig(localConfig);
      message = 'Local config saved!';
      setTimeout(() => message = '', 3000);
    } catch (e) {
      message = 'Failed to save local config';
    }
  }
</script>

<div class="max-w-4xl mx-auto p-6 space-y-8">
  <h1 class="text-2xl font-bold text-gray-800">Settings</h1>

  {#if isLoading}
    <div class="text-center py-12">Loading settings...</div>
  {:else}
    <section class="bg-white shadow rounded-lg p-6">
      <h2 class="text-xl font-semibold mb-4 border-b pb-2">Global Settings</h2>
      <div class="space-y-4">
        <div>
          <label class="block text-sm font-medium text-gray-700">Visualizer Position</label>
          <select
            bind:value={globalConfig.visualizerPosition}
            class="mt-1 block w-full pl-3 pr-10 py-2 text-base border-gray-300 focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm rounded-md"
          >
            <option value="left">Left</option>
            <option value="right">Right</option>
          </select>
        </div>
        <div>
          <label class="block text-sm font-medium text-gray-700">Default Visualizer URL</label>
          <input
            type="url"
            bind:value={globalConfig.visualizerUrl}
            placeholder="https://..."
            class="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm"
          />
        </div>
        <div class="grid grid-cols-2 gap-4">
          <div>
            <label class="block text-sm font-medium text-gray-700">Default Seed</label>
            <input
              type="number"
              bind:value={globalConfig.defaultSeed}
              class="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm"
            />
          </div>
          <div>
            <label class="block text-sm font-medium text-gray-700">Default Scale</label>
            <input
              type="number"
              step="0.1"
              bind:value={globalConfig.defaultScale}
              class="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm"
            />
          </div>
        </div>
        <button
          onclick={saveGlobal}
          class="inline-flex justify-center py-2 px-4 border border-transparent shadow-sm text-sm font-medium rounded-md text-white bg-indigo-600 hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500"
        >
          Save Global Config
        </button>
      </div>
    </section>

    <section class="bg-white shadow rounded-lg p-6">
      <h2 class="text-xl font-semibold mb-4 border-b pb-2">Local Project Settings</h2>
      <div class="space-y-4">
        <div>
          <label class="block text-sm font-medium text-gray-700">Project-specific Visualizer URL</label>
          <input
            type="url"
            bind:value={localConfig.visualizerUrl}
            placeholder="Leave empty to use global"
            class="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm"
          />
        </div>
        <button
          onclick={saveLocal}
          class="inline-flex justify-center py-2 px-4 border border-transparent shadow-sm text-sm font-medium rounded-md text-white bg-indigo-600 hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500"
        >
          Save Local Config
        </button>
      </div>
    </section>
  {/if}

  {#if message}
    <div class="fixed bottom-4 right-4 bg-gray-800 text-white px-4 py-2 rounded shadow-lg transition-opacity">
      {message}
    </div>
  {/if}
</div>

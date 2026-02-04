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
      showMessage('Global config saved!');
    } catch (e) {
      showMessage('Failed to save global config', true);
    }
  }

  async function saveLocal() {
    try {
      await api.saveLocalConfig(localConfig);
      showMessage('Local config saved!');
    } catch (e) {
      showMessage('Failed to save local config', true);
    }
  }

  function showMessage(msg: string, isError = false) {
    message = msg;
    // Basic error handling visual cue could be added here
    setTimeout(() => message = '', 3000);
  }
</script>

<div class="h-full overflow-y-auto bg-gray-50 p-6 md:p-8">
  <div class="max-w-none space-y-8">
    
    <div class="flex items-center justify-between">
      <h1 class="text-2xl font-bold text-gray-900 tracking-tight">Settings</h1>
      {#if message}
        <div class="bg-gray-900 text-white px-4 py-2 rounded-md shadow-lg text-sm font-medium animate-fade-in-up">
            {message}
        </div>
      {/if}
    </div>

    {#if isLoading}
      <div class="flex items-center justify-center py-12 text-gray-500">
        <svg class="animate-spin -ml-1 mr-3 h-5 w-5 text-indigo-500" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Loading settings...
      </div>
    {:else}
      <div class="grid grid-cols-1 xl:grid-cols-2 gap-8">
        
        <!-- Global Settings -->
        <section class="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
          <div class="px-6 py-4 border-b border-gray-200 bg-gray-50 flex justify-between items-center">
             <h2 class="text-lg font-semibold text-gray-900">Global Settings</h2>
             <span class="text-xs font-medium text-gray-500 uppercase tracking-wider">User Preferences</span>
          </div>
          
          <div class="p-6 space-y-6">
            <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                 <div class="col-span-1 md:col-span-2">
                    <label class="block text-sm font-medium text-gray-700 mb-1">Visualizer Position</label>
                    <select
                        bind:value={globalConfig.visualizerPosition}
                        class="block w-full pl-3 pr-10 py-2 text-base border-gray-300 focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm rounded-md shadow-sm transition-shadow"
                    >
                        <option value="left">Left (Split View)</option>
                        <option value="right">Right (Split View)</option>
                    </select>
                    <p class="mt-1 text-xs text-gray-500">Controls where the visualizer appears in the History tab.</p>
                </div>

                <div class="col-span-1 md:col-span-2">
                    <label class="block text-sm font-medium text-gray-700 mb-1">Default Visualizer URL</label>
                    <input
                        type="url"
                        bind:value={globalConfig.visualizerUrl}
                        placeholder="https://img.atcoder.jp/..."
                        class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
                    />
                    <p class="mt-1 text-xs text-gray-500">Fallback URL if no local project setting is defined.</p>
                </div>

                <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">Default Seed</label>
                    <input
                        type="number"
                        bind:value={globalConfig.defaultSeed}
                        class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
                    />
                </div>
                
                <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">Default Scale</label>
                    <input
                        type="number"
                        step="0.1"
                        bind:value={globalConfig.defaultScale}
                        class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
                    />
                </div>
            </div>

            <div class="pt-4 border-t border-gray-100 flex justify-end">
                <button
                    onclick={saveGlobal}
                    class="inline-flex justify-center py-2 px-6 border border-transparent shadow-sm text-sm font-medium rounded-md text-white bg-indigo-600 hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-colors transform active:scale-95"
                >
                    Save Global Config
                </button>
            </div>
          </div>
        </section>

        <!-- Local Settings -->
        <section class="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden h-fit">
          <div class="px-6 py-4 border-b border-gray-200 bg-gray-50 flex justify-between items-center">
             <h2 class="text-lg font-semibold text-gray-900">Project Settings</h2>
             <span class="text-xs font-medium text-gray-500 uppercase tracking-wider">Local Config</span>
          </div>

          <div class="p-6 space-y-6">
            <div>
              <label class="block text-sm font-medium text-gray-700 mb-1">Project-specific Visualizer URL</label>
              <input
                type="url"
                bind:value={localConfig.visualizerUrl}
                placeholder="Leave empty to use global default"
                class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
              />
              <p class="mt-2 text-xs text-gray-500">Overrides the global visualizer URL for this specific project.</p>
            </div>

            <div class="pt-4 border-t border-gray-100 flex justify-end">
              <button
                onclick={saveLocal}
                class="inline-flex justify-center py-2 px-6 border border-transparent shadow-sm text-sm font-medium rounded-md text-white bg-indigo-600 hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-colors transform active:scale-95"
              >
                Save Local Config
              </button>
            </div>
          </div>
        </section>
      
      </div>
    {/if}
  </div>
</div>

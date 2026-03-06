<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type GlobalConfig, type LocalConfig } from './api';

  let globalConfig = $state<GlobalConfig>({
    visualizerPosition: 'right',
    visualizerUrl: '',
    defaultSeed: 0,
    defaultScale: 1.0,
    testRunOptions: {
      shuffle: false,
      settingFile: 'pahcer_config.toml',
      freezeBestScores: false,
      noCompile: false
    }
  });

  let localConfig = $state<LocalConfig>({
    visualizerUrl: ''
  });

  let initialGlobalConfig = $state<GlobalConfig | null>(null);
  let initialLocalConfig = $state<LocalConfig | null>(null);

  let scalePercent = $state(100);
  let isLoading = $state(true);
  let isSaving = $state(false);
  let message = $state('');

  onMount(async () => {
    try {
      const res = await api.getConfig();
      globalConfig = { 
        ...globalConfig, 
        ...res.global,
        testRunOptions: {
          shuffle: res.global.testRunOptions?.shuffle ?? globalConfig.testRunOptions!.shuffle,
          settingFile: res.global.testRunOptions?.settingFile ?? globalConfig.testRunOptions!.settingFile,
          freezeBestScores: res.global.testRunOptions?.freezeBestScores ?? globalConfig.testRunOptions!.freezeBestScores,
          noCompile: res.global.testRunOptions?.noCompile ?? globalConfig.testRunOptions!.noCompile,
        }
      };
      localConfig = { ...localConfig, ...res.local };
      
      // Store initial state for comparison
      initialGlobalConfig = JSON.parse(JSON.stringify(globalConfig));
      initialLocalConfig = JSON.parse(JSON.stringify(localConfig));
      
      // ロードした値をパーセントに変換
      scalePercent = Math.round(globalConfig.defaultScale * 100);
    } catch (e) {
      console.error('Failed to load config', e);
    } finally {
      isLoading = false;
    }
  });

  // Check for unsaved changes
  let hasUnsavedChanges = $derived.by(() => {
    if (!initialGlobalConfig || !initialLocalConfig) return false;

    // Create a temporary global config with the current scalePercent for comparison
    const currentGlobal = {
        ...globalConfig,
        defaultScale: scalePercent / 100
    };

    const globalChanged = JSON.stringify(currentGlobal) !== JSON.stringify(initialGlobalConfig);
    const localChanged = JSON.stringify(localConfig) !== JSON.stringify(initialLocalConfig);

    return globalChanged || localChanged;
  });

  async function saveGlobal() {
    try {
      // パーセントを小数に戻して保存
      globalConfig.defaultScale = scalePercent / 100;
      await api.saveGlobalConfig(globalConfig);
      initialGlobalConfig = JSON.parse(JSON.stringify(globalConfig));
      showMessage('Global config saved!');
    } catch (e) {
      showMessage('Failed to save global config', true);
    }
  }

  async function saveLocal() {
    isSaving = true;
    try {
      if (localConfig.visualizerUrl) {
        const { exists } = await api.getVisualizerStatus();
        if (exists) {
          if (!confirm('Existing visualizer files found. Overwrite?')) {
            isSaving = false;
            return;
          }
        }
        
        showMessage('Downloading visualizer and assets...');
        await api.downloadVisualizer(localConfig.visualizerUrl);
      }
      
      await api.saveLocalConfig(localConfig);
      initialLocalConfig = JSON.parse(JSON.stringify(localConfig));
      showMessage('Local config saved and visualizer downloaded!');
    } catch (e) {
      console.error(e);
      showMessage('Error occurred during save/download', true);
    } finally {
      isSaving = false;
    }
  }

  function showMessage(msg: string, isError = false) {
    message = msg;
    setTimeout(() => message = '', 5000);
  }
</script>

<div class="h-full overflow-y-auto bg-gray-50 p-6">
  <div class="max-w-none space-y-8">
    
    <div class="flex items-center space-x-4">
      <h1 class="text-2xl font-bold text-gray-900 tracking-tight">Settings</h1>
      
      <div class="flex-1 flex items-center space-x-4">
        {#if hasUnsavedChanges}
            <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-yellow-100 text-yellow-800 animate-pulse">
                Unsaved changes
            </span>
        {/if}

        {#if message}
            <div class="bg-gray-900 text-white px-4 py-2 rounded-md shadow-lg text-sm font-medium animate-fade-in-up">
                {message}
            </div>
        {/if}
      </div>
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

                <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">Default Seed</label>
                    <input
                        type="number"
                        bind:value={globalConfig.defaultSeed}
                        class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
                    />
                </div>
                
                <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">Default Scale (%)</label>
                    <div class="relative">
                        <input
                            type="number"
                            step="5"
                            bind:value={scalePercent}
                            class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow pr-8"
                        />
                        <span class="absolute inset-y-0 right-3 flex items-center text-gray-500 pointer-events-none sm:text-sm">%</span>
                    </div>
                </div>
            </div>

            <!-- Test Run Options -->
            <div class="pt-6 border-t border-gray-100 space-y-4">
                <h3 class="text-sm font-bold text-gray-400 uppercase tracking-widest">Default Test Run Options</h3>
                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div>
                        <label class="block text-sm font-medium text-gray-700 mb-1">Default Setting File</label>
                        <input
                            type="text"
                            bind:value={globalConfig.testRunOptions!.settingFile}
                            placeholder="pahcer_config.toml"
                            class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
                        />
                    </div>
                </div>
                <div class="grid grid-cols-2 md:grid-cols-3 gap-4">
                    <label class="flex items-center space-x-3 cursor-pointer group">
                        <input type="checkbox" bind:checked={globalConfig.testRunOptions!.shuffle} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />
                        <span class="text-sm text-gray-600 group-hover:text-gray-900">Shuffle cases</span>
                    </label>
                    <label class="flex items-center space-x-3 cursor-pointer group">
                        <input type="checkbox" bind:checked={globalConfig.testRunOptions!.freezeBestScores} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />
                        <span class="text-sm text-gray-600 group-hover:text-gray-900">Freeze Best Scores</span>
                    </label>
                    <label class="flex items-center space-x-3 cursor-pointer group">
                        <input type="checkbox" bind:checked={globalConfig.testRunOptions!.noCompile} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />
                        <span class="text-sm text-gray-600 group-hover:text-gray-900">No Compile</span>
                    </label>
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
              <label class="block text-sm font-medium text-gray-700 mb-1">Project Visualizer URL</label>
              <input
                type="url"
                bind:value={localConfig.visualizerUrl}
                placeholder="https://img.atcoder.jp/..."
                class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
              />
              <p class="mt-2 text-xs text-gray-500">The URL of the visualizer for this specific project.</p>
            </div>

            <div class="pt-4 border-t border-gray-100 flex justify-end">
              <button
                onclick={saveLocal}
                disabled={isSaving}
                class="inline-flex justify-center py-2 px-6 border border-transparent shadow-sm text-sm font-medium rounded-md text-white bg-indigo-600 hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-colors transform active:scale-95 disabled:opacity-50"
              >
                {isSaving ? 'Downloading...' : 'Save Local Config'}
              </button>
            </div>
          </div>
        </section>
      
      </div>
    {/if}
  </div>
</div>

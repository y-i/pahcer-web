<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type ConfigResponse,
    DEFAULT_TEST_RUN_OPTIONS,
    type GlobalConfig,
    type LocalConfig,
    type VisualizerInitialScrollPosition,
  } from './api';
  import {
    getNotificationSupportState,
    requestNotificationAccess,
    type NotificationSupportState,
  } from './notifications';
  import { syncSavedTestRunDefaults } from './testRunState';

  interface Props {
    initialConfig: ConfigResponse;
    onConfigChange: (config: ConfigResponse) => void;
  }

  let { initialConfig, onConfigChange }: Props = $props();
  let configSnapshot = $state<ConfigResponse | null>(null);

  let globalConfig = $state<GlobalConfig>({
    visualizerPosition: 'right',
    visualizerInitialScrollPosition: 'bottom',
    visualizerUrl: '',
    resultJsonMode: 'symlink',
    defaultSeed: 0,
    defaultScale: 1.0,
    testRunOptions: {
      shuffle: false,
      settingFile: 'pahcer_config.toml',
      freezeBestScores: false,
      noCompile: false
    },
    notifications: {
      testRunCompleted: false,
    },
  });

  let localConfig = $state<LocalConfig>({
    visualizerUrl: ''
  });

  let initialGlobalConfig = $state<GlobalConfig | null>(null);
  let initialLocalConfig = $state<LocalConfig | null>(null);

  let scalePercent = $state(100);
  let isLoading = $state(true);
  let isSaving = $state(false);
  let isRequestingNotificationPermission = $state(false);
  let message = $state('');
  let notificationPermission = $state<NotificationSupportState>('unsupported');

  const visualizerInitialScrollOptions: Array<{ value: VisualizerInitialScrollPosition; label: string }> = [
    { value: 'top', label: '先頭から表示する' },
    { value: 'bottom', label: '末尾から表示する' },
  ];

  $effect(() => {
    configSnapshot = initialConfig;
  });

  onMount(async () => {
    notificationPermission = getNotificationSupportState();

    globalConfig = { 
      ...globalConfig, 
      ...initialConfig.global,
      testRunOptions: {
        shuffle: initialConfig.global.testRunOptions?.shuffle ?? globalConfig.testRunOptions!.shuffle,
        settingFile: initialConfig.global.testRunOptions?.settingFile ?? globalConfig.testRunOptions!.settingFile,
        freezeBestScores: initialConfig.global.testRunOptions?.freezeBestScores ?? globalConfig.testRunOptions!.freezeBestScores,
        noCompile: initialConfig.global.testRunOptions?.noCompile ?? globalConfig.testRunOptions!.noCompile,
      },
      notifications: {
        testRunCompleted: initialConfig.global.notifications?.testRunCompleted ?? globalConfig.notifications!.testRunCompleted,
      },
    };
    localConfig = { ...localConfig, ...initialConfig.local };
    syncSavedTestRunDefaults({
      shuffle: initialConfig.global.testRunOptions?.shuffle ?? DEFAULT_TEST_RUN_OPTIONS.shuffle,
      settingFile: initialConfig.global.testRunOptions?.settingFile ?? DEFAULT_TEST_RUN_OPTIONS.settingFile,
      freezeBestScores: initialConfig.global.testRunOptions?.freezeBestScores ?? DEFAULT_TEST_RUN_OPTIONS.freezeBestScores,
      noCompile: initialConfig.global.testRunOptions?.noCompile ?? DEFAULT_TEST_RUN_OPTIONS.noCompile,
    });
    
    initialGlobalConfig = JSON.parse(JSON.stringify(globalConfig));
    initialLocalConfig = JSON.parse(JSON.stringify(localConfig));
    scalePercent = Math.round(globalConfig.defaultScale * 100);
    isLoading = false;
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
      const nextGlobalConfig = {
        ...globalConfig,
        defaultScale: scalePercent / 100,
      };
      await api.saveGlobalConfig(nextGlobalConfig);
      globalConfig = nextGlobalConfig;
      syncSavedTestRunDefaults({
        shuffle: nextGlobalConfig.testRunOptions?.shuffle ?? DEFAULT_TEST_RUN_OPTIONS.shuffle,
        settingFile: nextGlobalConfig.testRunOptions?.settingFile ?? DEFAULT_TEST_RUN_OPTIONS.settingFile,
        freezeBestScores: nextGlobalConfig.testRunOptions?.freezeBestScores ?? DEFAULT_TEST_RUN_OPTIONS.freezeBestScores,
        noCompile: nextGlobalConfig.testRunOptions?.noCompile ?? DEFAULT_TEST_RUN_OPTIONS.noCompile,
      });
      initialGlobalConfig = JSON.parse(JSON.stringify(nextGlobalConfig));
      const baseConfig = configSnapshot ?? initialConfig;
      configSnapshot = {
        ...baseConfig,
        global: nextGlobalConfig,
      };
      onConfigChange(configSnapshot);
      showMessage('Global config saved!');
    } catch (e) {
      showMessage(e instanceof Error ? e.message : 'Failed to save global config', true);
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
      const baseConfig = configSnapshot ?? initialConfig;
      configSnapshot = {
        ...baseConfig,
        local: { ...localConfig },
      };
      onConfigChange(configSnapshot);
      showMessage('Local config saved and visualizer downloaded!');
    } catch (e) {
      console.error(e);
      showMessage(e instanceof Error ? e.message : 'Error occurred during save/download', true);
    } finally {
      isSaving = false;
    }
  }

  function showMessage(msg: string, isError = false) {
    message = msg;
    setTimeout(() => message = '', 5000);
  }

  async function handleNotificationPermissionRequest() {
    isRequestingNotificationPermission = true;

    try {
      notificationPermission = await requestNotificationAccess();
      if (notificationPermission === 'granted') {
        showMessage('Notification permission granted.');
      } else if (notificationPermission === 'denied') {
        showMessage('Notification permission denied.', true);
      } else if (notificationPermission === 'default') {
        showMessage('Notification permission request was dismissed.');
      }
    } catch (e) {
      showMessage('Failed to request notification permission.', true);
    } finally {
      isRequestingNotificationPermission = false;
    }
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
                  <label class="block text-sm font-medium text-gray-700 mb-1" for="visualizer-position">Visualizer Position</label>
                    <select
                    id="visualizer-position"
                        bind:value={globalConfig.visualizerPosition}
                        class="block w-full pl-3 pr-10 py-2 text-base border-gray-300 focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm rounded-md shadow-sm transition-shadow"
                    >
                        <option value="left">Left (Split View)</option>
                        <option value="right">Right (Split View)</option>
                    </select>
                    <p class="mt-1 text-xs text-gray-500">Controls where the visualizer appears in the History tab.</p>
                </div>

                <div class="col-span-1 md:col-span-2">
                  <label class="block text-sm font-medium text-gray-700 mb-1" for="visualizer-initial-scroll-position">ビジュアライザ初期スクロール位置</label>
                  <select
                    id="visualizer-initial-scroll-position"
                    bind:value={globalConfig.visualizerInitialScrollPosition}
                    class="block w-full pl-3 pr-10 py-2 text-base border-gray-300 focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm rounded-md shadow-sm transition-shadow"
                  >
                    {#each visualizerInitialScrollOptions as option}
                      <option value={option.value}>{option.label}</option>
                    {/each}
                  </select>
                  <p class="mt-1 text-xs text-gray-500">履歴タブでビジュアライザを開いたときの表示開始位置を選びます。</p>
                </div>

                <div class="col-span-1 md:col-span-2">
                  <div class="block text-sm font-medium text-gray-700 mb-2">Result JSON Placement</div>
                  <div class="space-y-3">
                    <label class="flex items-start space-x-3 cursor-pointer group">
                      <input
                        type="radio"
                        name="result-json-mode"
                        value="symlink"
                        bind:group={globalConfig.resultJsonMode}
                        class="mt-1 h-4 w-4 text-indigo-600 border-gray-300 focus:ring-indigo-500 transition-colors"
                      />
                      <div class="flex-1">
                        <span class="text-sm text-gray-700 font-medium">Symlink</span>
                        <p class="text-xs text-gray-500 mt-0.5">Creates a symbolic link to the original JSON file (recommended for most environments).</p>
                      </div>
                    </label>
                    <label class="flex items-start space-x-3 cursor-pointer group">
                      <input
                        type="radio"
                        name="result-json-mode"
                        value="copy"
                        bind:group={globalConfig.resultJsonMode}
                        class="mt-1 h-4 w-4 text-indigo-600 border-gray-300 focus:ring-indigo-500 transition-colors"
                      />
                      <div class="flex-1">
                        <span class="text-sm text-gray-700 font-medium">Copy</span>
                        <p class="text-xs text-gray-500 mt-0.5">Copies the JSON content as a regular file. Use this if symlinks are not supported on your system.</p>
                      </div>
                    </label>
                  </div>
                </div>

                <div>
          <label class="block text-sm font-medium text-gray-700 mb-1" for="default-seed">Default Seed</label>
                    <input
            id="default-seed"
                        type="number"
                        bind:value={globalConfig.defaultSeed}
                        class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
                    />
                </div>
                
                <div>
                  <label class="block text-sm font-medium text-gray-700 mb-1" for="default-scale">Default Scale (%)</label>
                    <div class="relative">
                        <input
                      id="default-scale"
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
                    <label class="block text-sm font-medium text-gray-700 mb-1" for="default-setting-file">Default Setting File</label>
                        <input
                      id="default-setting-file"
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

                <div class="pt-6 border-t border-gray-100 space-y-4">
                  <div class="flex items-start justify-between gap-4">
                    <div class="space-y-1">
                      <h3 class="text-sm font-bold text-gray-400 uppercase tracking-widest">Notifications</h3>
                      <p class="text-sm text-gray-500">Use this default for the next run. Permission requests only happen from the button below.</p>
                    </div>
                    <span class="inline-flex items-center rounded-full px-2.5 py-1 text-xs font-medium
                      {notificationPermission === 'granted' ? 'bg-green-100 text-green-700' : notificationPermission === 'unsupported' ? 'bg-gray-100 text-gray-600' : notificationPermission === 'denied' ? 'bg-red-100 text-red-700' : 'bg-yellow-100 text-yellow-700'}">
                      {notificationPermission === 'granted'
                        ? 'Permission granted'
                        : notificationPermission === 'denied'
                        ? 'Permission denied'
                        : notificationPermission === 'default'
                          ? 'Permission not requested'
                          : 'Not supported'}
                    </span>
                  </div>

                  <label class="flex items-center space-x-3 cursor-pointer group">
                    <input type="checkbox" bind:checked={globalConfig.notifications!.testRunCompleted} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />
                    <span class="text-sm text-gray-600 group-hover:text-gray-900">Enable desktop notification by default when a run completes</span>
                  </label>

                  <div class="flex items-center justify-between gap-4 rounded-lg border border-gray-200 bg-gray-50 px-4 py-3">
                    <p class="text-sm text-gray-600">If notifications are blocked or unsupported, runs still finish normally and no notification is shown.</p>
                    <button
                      onclick={handleNotificationPermissionRequest}
                      disabled={notificationPermission === 'unsupported' || isRequestingNotificationPermission}
                      class="inline-flex justify-center py-2 px-4 border border-gray-300 shadow-sm text-sm font-medium rounded-md text-gray-700 bg-white hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-colors disabled:opacity-50"
                    >
                      {isRequestingNotificationPermission ? 'Requesting...' : 'Request Permission'}
                    </button>
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
              <label class="block text-sm font-medium text-gray-700 mb-1" for="project-visualizer-url">Project Visualizer URL</label>
              <input
                id="project-visualizer-url"
                type="url"
                bind:value={localConfig.visualizerUrl}
                placeholder="https://img.atcoder.jp/..."
                class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
              />
              <p class="mt-2 text-xs text-gray-500">The URL of the visualizer for this specific project.</p>
            </div>

            <div>
              <label class="block text-sm font-medium text-gray-700 mb-1" for="default-score-type">Default Score Type</label>
              <select
                id="default-score-type"
                bind:value={localConfig.defaultScoreType}
                class="block w-full pl-3 pr-10 py-2 text-base border-gray-300 focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm rounded-md shadow-sm transition-shadow"
              >
                <option value="">Unspecified</option>
                <option value="raw">Raw Score</option>
                <option value="max">Relative (YOUR/MAX)</option>
                <option value="min">Relative (MIN/YOUR)</option>
                <option value="rank_max">Rank (Higher is Better)</option>
                <option value="rank_min">Rank (Lower is Better)</option>
              </select>
              <p class="mt-2 text-xs text-gray-500">The default score calculation method for the analysis tool.</p>
            </div>

            <div>
              <label class="block text-sm font-medium text-gray-700 mb-1" for="input-parameter-names">Input Parameter Names (CSV)</label>
              <input
                id="input-parameter-names"
                type="text"
                bind:value={localConfig.inputParamNames}
                placeholder="N,M,L,K"
                class="block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm transition-shadow"
              />
              <p class="mt-2 text-xs text-gray-500">Custom column names for parameters in score analysis (e.g., "N,M,L,K").</p>
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

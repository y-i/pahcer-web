<script lang="ts">
  import { api, type ConfigResponse, type InitLanguage, type InitObjective } from './api';

  interface Props {
    config: ConfigResponse;
    onSuccess: (config: ConfigResponse) => void;
  }

  let { config, onSuccess }: Props = $props();

  let problem = $state('');
  let objective = $state<InitObjective>('max');
  let language = $state<InitLanguage>('rust');
  let interactive = $state(false);
  let isSubmitting = $state(false);
  let errorMessage = $state('');

  const languageOptions: Array<{ value: InitLanguage; label: string }> = [
    { value: 'cpp', label: 'C++' },
    { value: 'python', label: 'Python' },
    { value: 'rust', label: 'Rust' },
    { value: 'go', label: 'Go' },
  ];

  async function submitInitialization() {
    if (isSubmitting) {
      return;
    }

    const normalizedProblem = problem.trim();
    if (!normalizedProblem) {
      errorMessage = 'problem を入力してください。';
      return;
    }

    isSubmitting = true;
    errorMessage = '';

    try {
      const config = await api.initProject({
        problem: normalizedProblem,
        objective,
        language,
        interactive,
      });
      onSuccess(config);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : '初期化に失敗しました。';
    } finally {
      isSubmitting = false;
    }
  }
</script>

<main class="flex-1 h-full overflow-auto bg-gray-50 p-6 flex items-center justify-center min-h-0">
  <div class="w-full max-w-2xl">
    <section class="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
      <div class="px-8 py-6 border-b border-gray-200 bg-gray-50">
        <div class="flex items-center justify-between">
          <h1 class="text-2xl font-bold text-gray-900 tracking-tight">セットアップを始めましょう</h1>
          <span class="text-xs font-medium text-gray-500 uppercase tracking-widest">Pahcer Setup</span>
        </div>
        <div class="mt-4 space-y-4">
          <div class="rounded-lg bg-indigo-50 px-4 py-3 border border-indigo-100">
            <p class="text-xs font-bold uppercase tracking-widest text-indigo-400 mb-1">Target Directory</p>
            <p class="font-mono text-sm break-all text-indigo-900">{config.baseDir}</p>
          </div>
          <p class="max-w-2xl text-sm leading-6 text-gray-600">
            このディレクトリで Pahcer を使用するための初期設定を行います。以下の項目を入力して、プロジェクトを開始してください。
          </p>
        </div>
      </div>

      <form class="p-8 space-y-6" onsubmit={(event) => {
        event.preventDefault();
        void submitInitialization();
      }}>
        <div class="grid gap-6 md:grid-cols-2">
          <label class="space-y-1.5 md:col-span-2">
            <span class="text-xs font-semibold text-gray-600 uppercase tracking-wider">problem</span>
            <input
              type="text"
              bind:value={problem}
              placeholder="例: ahc061"
              class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-lg text-sm shadow-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-all outline-none"
              disabled={isSubmitting}
              required
            />
          </label>

          <label class="space-y-1.5">
            <span class="text-xs font-semibold text-gray-600 uppercase tracking-wider">objective</span>
            <select
              bind:value={objective}
              class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-lg text-sm shadow-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-all outline-none"
              disabled={isSubmitting}
            >
              <option value="max">max</option>
              <option value="min">min</option>
            </select>
          </label>

          <label class="space-y-1.5">
            <span class="text-xs font-semibold text-gray-600 uppercase tracking-wider">language</span>
            <select
              bind:value={language}
              class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-lg text-sm shadow-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-all outline-none"
              disabled={isSubmitting}
            >
              {#each languageOptions as option}
                <option value={option.value}>{option.label}</option>
              {/each}
            </select>
          </label>
        </div>

        <label class="flex items-start gap-3 rounded-lg border border-gray-200 bg-gray-50 px-4 py-4 text-sm text-gray-700 cursor-pointer group hover:bg-gray-100 transition-colors">
          <input
            type="checkbox"
            bind:checked={interactive}
            class="mt-1 h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors"
            disabled={isSubmitting}
          />
          <div class="flex flex-col">
            <span class="font-medium text-gray-900">interactive を有効にする</span>
            <span class="text-xs text-gray-500 leading-5">
              インタラクティブ問題の場合だけ有効にしてください。
            </span>
          </div>
        </label>

        {#if errorMessage}
          <div class="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 font-medium">
            {errorMessage}
          </div>
        {/if}

        <div class="flex items-center justify-between gap-4 border-t border-gray-100 pt-6">
          <p class="text-xs text-gray-500 font-medium">
            必須項目を入力して初期化を実行してください。
          </p>
          <button
            type="submit"
            class="inline-flex justify-center py-2 px-8 border border-transparent shadow-sm text-sm font-medium rounded-lg text-white bg-indigo-600 hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-all transform active:scale-95 disabled:opacity-50 disabled:cursor-not-allowed"
            disabled={isSubmitting || !problem.trim()}
          >
            {isSubmitting ? '初期化中...' : '初期化を実行'}
          </button>
        </div>
      </form>
    </section>
  </div>
</main>

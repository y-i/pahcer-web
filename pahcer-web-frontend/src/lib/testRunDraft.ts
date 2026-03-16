import {
  DEFAULT_TEST_RUN_OPTIONS,
  type PersistedTestRunOptions,
  type TestRunOptions,
} from './api';

export function extractPersistedTestRunOptions(
  options: TestRunOptions | PersistedTestRunOptions,
): PersistedTestRunOptions {
  return {
    shuffle: options.shuffle,
    settingFile: options.settingFile,
    freezeBestScores: options.freezeBestScores,
    noCompile: options.noCompile,
  };
}

export function normalizePersistedTestRunOptions(
  options?: PersistedTestRunOptions,
): PersistedTestRunOptions {
  return {
    ...extractPersistedTestRunOptions(DEFAULT_TEST_RUN_OPTIONS),
    ...options,
  };
}

export function arePersistedTestRunOptionsEqual(
  left: PersistedTestRunOptions,
  right: PersistedTestRunOptions,
): boolean {
  return left.shuffle === right.shuffle
    && left.settingFile === right.settingFile
    && left.freezeBestScores === right.freezeBestScores
    && left.noCompile === right.noCompile;
}

interface SyncDraftWithSavedDefaultsParams {
  draft: TestRunOptions;
  previousSavedDefaults: PersistedTestRunOptions;
  nextSavedDefaults: PersistedTestRunOptions;
  hasHydratedDraft: boolean;
  isRunning: boolean;
}

export function syncDraftWithSavedDefaults({
  draft,
  previousSavedDefaults,
  nextSavedDefaults,
  hasHydratedDraft,
  isRunning,
}: SyncDraftWithSavedDefaultsParams): TestRunOptions {
  const draftPersistedOptions = extractPersistedTestRunOptions(draft);
  const followsPreviousDefaults = arePersistedTestRunOptionsEqual(
    draftPersistedOptions,
    previousSavedDefaults,
  );

  if (!isRunning && (!hasHydratedDraft || followsPreviousDefaults)) {
    return {
      ...draft,
      ...nextSavedDefaults,
    };
  }

  return draft;
}
import { get, writable } from 'svelte/store';
import {
  api,
  DEFAULT_NOTIFICATION_SETTINGS,
  DEFAULT_TEST_RUN_OPTIONS,
  type GlobalConfig,
  type PersistedTestRunOptions,
  type RunLogEntry,
  type RunStreamEvent,
  type RunTerminationReason,
  type TestRunNotificationSettings,
  type TestRunOptions,
} from './api';
import {
  getNotificationSupportState,
  sendRunCompleteNotification,
  type NotificationSupportState,
} from './notifications';
import {
  extractPersistedTestRunOptions,
  normalizePersistedTestRunOptions,
  syncDraftWithSavedDefaults,
} from './testRunDraft';

interface TestRunState {
  isRunning: boolean;
  status: 'idle' | 'running' | 'canceling' | 'succeeded' | 'failed' | 'canceled';
  logs: RunLogEntry[];
  currentCommand: string;
  lastExitCode: number | null;
  options: TestRunOptions;
  notifications: {
    defaultEnabled: boolean;
    currentEnabled: boolean;
    isCustomized: boolean;
    permission: NotificationSupportState;
  };
  activeRunId: number;
  activeRunRequestId: string | null;
  lastNotifiedRunId: number | null;
}

const initialState: TestRunState = {
  isRunning: false,
  status: 'idle',
  logs: [],
  currentCommand: '',
  lastExitCode: null,
  options: { ...DEFAULT_TEST_RUN_OPTIONS },
  notifications: {
    defaultEnabled: DEFAULT_NOTIFICATION_SETTINGS.testRunCompleted,
    currentEnabled: DEFAULT_NOTIFICATION_SETTINGS.testRunCompleted,
    isCustomized: false,
    permission: getNotificationSupportState(),
  },
  activeRunId: 0,
  activeRunRequestId: null,
  lastNotifiedRunId: null,
};

export const testRunState = writable<TestRunState>(initialState);

let savedDefaults = extractPersistedTestRunOptions(DEFAULT_TEST_RUN_OPTIONS);
let hasHydratedDraft = false;

export function hydrateTestRunState(globalConfig: GlobalConfig): void {
  testRunState.update((state) => {
    const defaultNotificationEnabled = globalConfig.notifications?.testRunCompleted ?? false;
    const nextSavedDefaults = normalizePersistedTestRunOptions(globalConfig.testRunOptions);
    const nextOptions = syncDraftWithSavedDefaults({
      draft: state.options,
      previousSavedDefaults: savedDefaults,
      nextSavedDefaults,
      hasHydratedDraft,
      isRunning: state.isRunning,
    });

    savedDefaults = nextSavedDefaults;
    hasHydratedDraft = true;

    return {
      ...state,
      options: nextOptions,
      notifications: {
        ...state.notifications,
        defaultEnabled: defaultNotificationEnabled,
        currentEnabled:
          state.notifications.isCustomized || state.isRunning
            ? state.notifications.currentEnabled
            : defaultNotificationEnabled,
        permission: getNotificationSupportState(),
      },
    };
  });
}

export function refreshNotificationPermission(): void {
  testRunState.update((state) => ({
    ...state,
    notifications: {
      ...state.notifications,
      permission: getNotificationSupportState(),
    },
  }));
}

export function setTestRunOption<K extends keyof TestRunOptions>(
  key: K,
  value: TestRunOptions[K],
): void {
  testRunState.update((state) => {
    const nextOptions: TestRunOptions = {
      ...state.options,
      [key]: value,
    };

    return {
      ...state,
      options: nextOptions,
    };
  });
}

export function setRunNotificationEnabled(enabled: boolean): void {
  testRunState.update((state) => {
    if (state.isRunning) {
      return state;
    }

    return {
      ...state,
      notifications: {
        ...state.notifications,
        currentEnabled: enabled,
        isCustomized: enabled !== state.notifications.defaultEnabled,
      },
    };
  });
}

export function clearRunLogs(): void {
  testRunState.update((state) => ({
    ...state,
    logs: [],
  }));
}

export function syncSavedTestRunDefaults(options: PersistedTestRunOptions): void {
  const nextSavedDefaults = normalizePersistedTestRunOptions(options);

  testRunState.update((state) => {
    const nextOptions = syncDraftWithSavedDefaults({
      draft: state.options,
      previousSavedDefaults: savedDefaults,
      nextSavedDefaults,
      hasHydratedDraft,
      isRunning: state.isRunning,
    });

    savedDefaults = nextSavedDefaults;
    hasHydratedDraft = true;

    return {
      ...state,
      options: nextOptions,
    };
  });
}

function buildArgs(options: TestRunOptions): string[] {
  const args: string[] = [];

  if (options.shuffle) args.push('--shuffle');
  if (options.comment) args.push('-c', options.comment);
  if (options.tag) args.push('-t', options.tag);
  if (options.settingFile && options.settingFile !== DEFAULT_TEST_RUN_OPTIONS.settingFile) {
    args.push('--setting-file', options.settingFile);
  }
  if (options.freezeBestScores) args.push('--freeze-best-scores');
  if (options.noCompile) args.push('--no-compile');

  return args;
}

function buildCommand(args: string[]): string {
  return ['pahcer', 'run', ...args].join(' ');
}

function createRunId(): string {
  if (typeof globalThis.crypto?.randomUUID === 'function') {
    return globalThis.crypto.randomUUID();
  }

  return `run-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

function appendRunLog(type: RunLogEntry['type'], text: string): void {
  testRunState.update((state) => ({
    ...state,
    logs: [...state.logs, { type, text }],
  }));
}

function resetNotificationSelection(defaults: TestRunNotificationSettings): void {
  testRunState.update((state) => ({
    ...state,
    notifications: {
      ...state.notifications,
      defaultEnabled: defaults.testRunCompleted,
      currentEnabled: defaults.testRunCompleted,
      isCustomized: false,
      permission: getNotificationSupportState(),
    },
  }));
}

function terminalStatusFromReason(reason: RunTerminationReason, code: number): TestRunState['status'] {
  if (reason === 'canceled') {
    return 'canceled';
  }

  return code === 0 ? 'succeeded' : 'failed';
}

function handleRunExit(event: Extract<RunStreamEvent, { type: 'exit' }>, runId: number, notifyOnCompletion: boolean): void {
  const nextStatus = terminalStatusFromReason(event.reason, event.code);
  appendRunLog(
    'info',
    event.reason === 'canceled' ? 'Run canceled.' : `Process exited with code ${event.code}`,
  );

  const snapshot = get(testRunState);
  const defaultEnabled = snapshot.notifications.defaultEnabled;

  testRunState.update((state) => ({
    ...state,
    isRunning: false,
    status: nextStatus,
    lastExitCode: event.reason === 'canceled' ? null : event.code,
    activeRunRequestId: null,
    options: {
      ...state.options,
      comment: '',
      tag: '',
    },
  }));

  if (event.reason !== 'canceled' && notifyOnCompletion && snapshot.lastNotifiedRunId !== runId) {
    const didNotify = sendRunCompleteNotification(snapshot.currentCommand, event.code, runId);
    if (didNotify) {
      testRunState.update((state) => ({
        ...state,
        lastNotifiedRunId: runId,
        notifications: {
          ...state.notifications,
          permission: getNotificationSupportState(),
        },
      }));
    } else {
      refreshNotificationPermission();
    }
  }

  resetNotificationSelection({ testRunCompleted: defaultEnabled });
}

export async function startTestRun(): Promise<void> {
  const snapshot = get(testRunState);
  if (snapshot.isRunning) {
    return;
  }

  const args = buildArgs(snapshot.options);
  const command = buildCommand(args);
  const runId = snapshot.activeRunId + 1;
  const runRequestId = createRunId();
  const notifyOnCompletion = snapshot.notifications.currentEnabled;

  testRunState.update((state) => ({
    ...state,
    isRunning: true,
    status: 'running',
    logs: [
      { type: 'info', text: 'Starting pahcer run...' },
      { type: 'info', text: `Command: ${command}` },
    ],
    currentCommand: command,
    lastExitCode: null,
    activeRunId: runId,
    activeRunRequestId: runRequestId,
  }));

  try {
    await api.runPahcer({ runId: runRequestId, args }, (event) => {
      if (event.type === 'stdout' || event.type === 'stderr') {
        appendRunLog(event.type, event.data);
        return;
      }

      if (event.runId !== runRequestId) {
        return;
      }

      handleRunExit(event, runId, notifyOnCompletion);
    });
  } catch (error) {
    appendRunLog('stderr', `Error: ${String(error)}`);

    const defaultEnabled = get(testRunState).notifications.defaultEnabled;
    testRunState.update((state) => ({
      ...state,
      isRunning: false,
      status: 'failed',
      lastExitCode: null,
      activeRunRequestId: null,
      options: {
        ...state.options,
        comment: '',
        tag: '',
      },
    }));
    resetNotificationSelection({ testRunCompleted: defaultEnabled });
  }
}

export async function cancelTestRun(): Promise<void> {
  const snapshot = get(testRunState);
  if (!snapshot.isRunning || snapshot.status !== 'running' || !snapshot.activeRunRequestId) {
    return;
  }

  const runRequestId = snapshot.activeRunRequestId;
  appendRunLog('info', 'Cancel requested...');
  testRunState.update((state) => ({
    ...state,
    status: 'canceling',
  }));

  try {
    await api.cancelRun(runRequestId);
  } catch (error) {
    appendRunLog('stderr', `Cancel failed: ${String(error)}`);
    testRunState.update((state) => {
      if (state.activeRunRequestId !== runRequestId || state.status !== 'canceling') {
        return state;
      }

      return {
        ...state,
        status: 'running',
      };
    });
  }
}

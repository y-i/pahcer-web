export type NotificationSupportState = NotificationPermission | 'unsupported';

export function getNotificationSupportState(): NotificationSupportState {
  if (typeof window === 'undefined' || !('Notification' in window)) {
    return 'unsupported';
  }

  return Notification.permission;
}

export async function requestNotificationAccess(): Promise<NotificationSupportState> {
  if (typeof window === 'undefined' || !('Notification' in window)) {
    return 'unsupported';
  }

  return Notification.requestPermission();
}

export function sendRunCompleteNotification(command: string, exitCode: number, runId: number): boolean {
  if (getNotificationSupportState() !== 'granted') {
    return false;
  }

  const title = exitCode === 0 ? 'Pahcer run completed' : 'Pahcer run finished with errors';
  const body = exitCode === 0
    ? command
    : `${command} (exit code: ${exitCode})`;

  new Notification(title, {
    body,
    tag: `pahcer-run-${runId}`,
  });

  return true;
}
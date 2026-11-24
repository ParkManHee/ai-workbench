import * as TaskManager from "expo-task-manager";
import * as Notifications from "expo-notifications";
import { PROGRESS_CHANNEL, progressAction } from "./progress";

const TASK = "awb-progress-push";

// 앱이 백그라운드·종료 상태면 WS 가 끊겨 진행을 못 보므로, 데몬의 data-only 푸시로 알림 한 장을 덮어쓴다
TaskManager.defineTask<Notifications.NotificationTaskPayload>(TASK, async ({ data }) => {
  if (!data || "actionIdentifier" in data) return;
  let payload: Record<string, unknown>;
  try {
    payload = JSON.parse(data.data?.dataString ?? "");
  } catch {
    return;
  }
  const a = progressAction(payload);
  if (!a) return;
  if (a.kind === "dismiss") {
    await Notifications.dismissNotificationAsync(a.identifier);
    return;
  }
  await Notifications.scheduleNotificationAsync({
    identifier: a.identifier,
    content: { title: a.title, data: payload, sticky: true, sound: false },
    trigger: { channelId: PROGRESS_CHANNEL },
  });
});

Notifications.registerTaskAsync(TASK).catch(() => {});

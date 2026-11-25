import { useCallback, useEffect, useMemo, useState } from "react";
import { ScrollView, StyleSheet, Text, TextInput, View } from "react-native";
import { HttpError, makeClient } from "../lib/api";
import { DAYS, nextLabel } from "../lib/schedule";
import type { PC, Schedule } from "../lib/types";
import { font, radius, space, TOUCH, useTheme, type Theme } from "../lib/theme";
import { Button, Chip, Icon, ListRow, SheetFrame } from "./ui";

const MODES: [string, string][] = [["default", "감독"], ["acceptEdits", "편집 자동"], ["auto", "자동"], ["bypassPermissions", "전체"]];
const modeLabel = (m: string) => MODES.find(([k]) => k === m)?.[1] ?? m;

function daysLabel(days: number[]): string {
  if (days.length === 7) return "매일";
  if (days.join() === "1,2,3,4,5") return "평일";
  if (days.join() === "0,6") return "주말";
  return days.map((d) => DAYS[d]).join("·");
}

/** 세션 화면 「예약」 칸 — 요일·시각마다 새 대화로 프롬프트를 실행한다. 옛 데몬(엔드포인트 없음)이면 그리지 않는다 */
export function ScheduleSection({ pc, project }: { pc: PC; project: string }) {
  const t = useTheme();
  const styles = useMemo(() => makeStyles(t), [t]);
  const client = useMemo(() => makeClient(pc.baseUrl, pc.token), [pc]);
  const [list, setList] = useState<Schedule[] | null>(null);
  const [view, setView] = useState<"list" | "form" | null>(null);
  const [days, setDays] = useState<number[]>([1, 2, 3, 4, 5]);
  const [time, setTime] = useState("09:00");
  const [prompt, setPrompt] = useState("");
  const [mode, setMode] = useState("auto");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setList(await client.schedules(project));
    } catch {
      // 401 은 세션 목록 쪽이 PC 를 지우고 나간다
      setList(null);
    }
  }, [client, project]);

  useEffect(() => { load(); }, [load]);

  if (!list) return null;

  const next = list.length ? Math.min(...list.map((s) => s.next_at)) : null;

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await client.addSchedule(project, { days, time: time.trim(), prompt: prompt.trim(), mode });
      setPrompt("");
      setView("list");
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "저장하지 못했습니다");
    } finally {
      setBusy(false);
    }
  }

  async function remove(s: Schedule) {
    try {
      await client.deleteSchedule(project, s.id);
    } catch (e) {
      // 404 = 이미 지워짐 — 목록만 새로 받는다
      if (!(e instanceof HttpError && e.status === 404)) setError("지우지 못했습니다. 다시 시도해 주세요");
    }
    await load();
  }

  // 닫으면(뒤로·바깥·취소) 쓰던 폼을 버린다 — 다음 「예약 추가」 는 빈 폼에서 시작
  function resetForm() {
    setDays([1, 2, 3, 4, 5]);
    setTime("09:00");
    setPrompt("");
    setMode("auto");
    setError(null);
  }

  const validTime = /^([01]\d|2[0-3]):[0-5]\d$/.test(time.trim());

  return (
    <>
      <ListRow
        title="예약 실행"
        subtitle={next ? `${list.length}개 · 다음 ${nextLabel(next)}` : "요일·시각마다 새 대화로 실행"}
        onPress={() => { setError(null); setView("list"); }}
        right={<Icon name="schedule" size={20} color={t.subtext} />}
      />
      <SheetFrame
        visible={!!view}
        title={view === "form" ? "예약 추가" : "예약 실행"}
        variant={view === "form" ? "dialog" : "bottom"}
        onClose={() => { resetForm(); setView(null); }}
      >
        {view === "list" ? (
          <>
            <Text style={styles.hint}>PC가 잠자거나 꺼져 있어 놓친 회차는 건너뛰고 다음 회차에 실행합니다. 끝나면 알림이 옵니다.</Text>
            {/* ListRow 가 자기 좌우 패딩을 갖는다 — 시트 패딩만큼 빼서 행 제목이 시트 제목과 같은 선에 선다 */}
            <ScrollView style={{ maxHeight: 360, marginHorizontal: -space.lg }}>
              {list.length === 0 ? <Text style={styles.empty}>예약이 없습니다.</Text> : null}
              {list.map((s) => (
                <ListRow
                  key={s.id}
                  title={s.prompt}
                  subtitle={`${daysLabel(s.days)} ${s.time} · ${modeLabel(s.mode)} · 다음 ${nextLabel(s.next_at)}`}
                  right={<Button title="삭제" size="sm" variant="dangerOutline" onPress={() => remove(s)} />}
                />
              ))}
            </ScrollView>
            {error ? <Text style={styles.error}>{error}</Text> : null}
            <Button title="예약 추가" icon="add" onPress={() => { setError(null); setView("form"); }} />
          </>
        ) : view === "form" ? (
          <>
            <Text style={styles.label}>요일</Text>
            <View style={styles.chips}>
              {DAYS.map((d, i) => (
                <Chip
                  key={d}
                  label={d}
                  selected={days.includes(i)}
                  onPress={() => setDays((v) => (v.includes(i) ? v.filter((x) => x !== i) : [...v, i]))}
                />
              ))}
            </View>
            <Text style={styles.label}>시각(PC 기준, 24시간)</Text>
            <TextInput
              style={styles.input}
              value={time}
              onChangeText={setTime}
              placeholder="09:00"
              placeholderTextColor={t.placeholder}
              keyboardType="numbers-and-punctuation"
              maxLength={5}
            />
            <Text style={styles.label}>프롬프트</Text>
            <TextInput
              style={[styles.input, styles.prompt]}
              value={prompt}
              onChangeText={setPrompt}
              placeholder="예: 어제 커밋을 요약해 줘"
              placeholderTextColor={t.placeholder}
              multiline
            />
            <Text style={styles.label}>권한</Text>
            <View style={styles.chips}>
              {MODES.map(([k, l]) => <Chip key={k} label={l} selected={mode === k} onPress={() => setMode(k)} />)}
            </View>
            {error ? <Text style={styles.error}>{error}</Text> : null}
            <View style={styles.buttons}>
              <Button title="취소" variant="secondary" onPress={() => { resetForm(); setView("list"); }} style={styles.button} />
              <Button
                title="저장"
                onPress={save}
                loading={busy}
                disabled={days.length === 0 || !validTime || !prompt.trim()}
                style={styles.button}
              />
            </View>
          </>
        ) : null}
      </SheetFrame>
    </>
  );
}

const makeStyles = (t: Theme) => StyleSheet.create({
  hint: { fontSize: font.size.sm, color: t.subtext },
  empty: { fontSize: font.size.sm, color: t.subtext, paddingVertical: space.md, textAlign: "center" },
  error: { fontSize: font.size.sm, color: t.danger },
  label: { fontSize: font.size.sm, color: t.subtext, fontWeight: font.weight.medium },
  chips: { flexDirection: "row", flexWrap: "wrap", gap: space.sm },
  input: {
    minHeight: TOUCH,
    paddingHorizontal: space.md,
    borderRadius: radius.md,
    borderWidth: 1,
    borderColor: t.border,
    fontSize: font.size.md,
    color: t.text,
  },
  prompt: { minHeight: 88, paddingVertical: space.sm, textAlignVertical: "top" },
  buttons: { flexDirection: "row", gap: space.sm, paddingTop: space.sm, paddingBottom: space.lg },
  button: { flex: 1 },
});

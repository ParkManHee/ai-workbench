// 종료 상태에서 푸시로 깨어나는 헤드리스 기동은 화면을 렌더하지 않아 app/ 모듈이 평가되지 않는다 — 태스크는 진입점에서 정의해야 한다
import "./src/lib/progressTask";
import "expo-router/entry";

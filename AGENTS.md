# AGENTS.md — 역할별 AI 에이전트 가이드

이 파일은 각 역할(Integrator, Data, Realtime)의 팀원이 AI 코딩 어시스턴트와 협업할 때 참고하는 가이드입니다. AI에게 작업을 요청하기 전에 자기 역할 섹션을 읽고, 필요하면 프롬프트에 이 파일의 내용을 참조하도록 안내하세요.

---

## 공통 규칙 (모든 역할)

### AI에게 반드시 알려줘야 할 것
- **자기 역할**: "나는 Data 담당이다" 등을 항상 명시하세요. AI가 소유권 규칙을 지킬 수 있습니다.
- **작업 범위**: 어떤 모듈의 어떤 함수를 구현하는지 구체적으로 알려주세요.
- **CLAUDE.md 참조**: AI가 이 프로젝트의 컨벤션을 모를 수 있으므로 "CLAUDE.md를 먼저 읽어라"고 지시하세요.

### AI에게 금지시켜야 할 것
- 자기 소유가 아닌 모듈 수정 금지
- `model/` 타입 변경 시 팀 합의 없이 진행 금지
- `Cargo.toml` 직접 수정 금지 (Integrator 전용)
- `unwrap()`/`expect()` 프로덕션 코드 사용 금지
- `cargo_metadata` 크레이트 사용 금지

### AI 작업 후 항상 확인할 것
```bash
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

### 프롬프트 템플릿

```
나는 cargo-chrono 프로젝트의 [역할] 담당이다.
CLAUDE.md와 AGENTS.md를 먼저 읽고 프로젝트 규칙을 따라라.

[작업 내용]

조건:
- 내 소유 모듈([모듈 목록])만 수정해라
- 다른 모듈은 절대 수정하지 마라
- model/ 타입은 있는 그대로 사용해라
- anyhow::Result로 에러를 반환해라
- 모든 public API에 doc comment를 작성해라
- 완료 후 cargo clippy -- -D warnings && cargo test가 통과하는지 확인해라
```

---

## Integrator 역할

### 소유 모듈
`src/model/`, `src/cli/`, `src/supervisor/`, `src/parser/`, `src/main.rs`, `Cargo.toml`

### 주요 구현 대상

#### supervisor::spawn_build()
```
src/supervisor/mod.rs의 spawn_build() 함수를 구현해라.

요구사항:
- tokio::process::Command로 `cargo build --message-format=json-render-diagnostics` 실행
- workspace_dir에서 실행, cargo_args를 추가 인수로 전달
- stdout을 BufReader로 감싸서 라인 단위로 읽기
- 각 라인을 mpsc::Sender<String>으로 전송 (bounded, 1024)
- stderr는 inherit (사용자에게 직접 보이도록)
- SupervisorHandle에 child process를 보관
- cancel()은 child.kill(), wait()은 child.wait()
```

#### parser::run_parser()
```
src/parser/mod.rs의 run_parser() 함수를 구현해라.

요구사항:
- mpsc::Receiver<String>에서 JSON 라인을 읽어 serde_json::from_str로 파싱
- Cargo JSON 메시지의 "reason" 필드로 분기:
  - "compiler-artifact" → CompilationFinished (내부에서 start/finish 매칭)
  - "compiler-message" → CompilerMessage
  - "build-finished" → BuildFinished
- 첫 이벤트로 BuildStarted를 직접 생성해서 발행 (ParserConfig의 정보 사용)
- 내부 HashMap<CrateId, Instant>으로 compilation start/finish 매칭
- 알 수 없는 reason은 무시 (forward compatibility)
- cargo_metadata 크레이트를 쓰지 말고 serde_json으로 직접 파싱
```

#### 주의사항
- `model/` 타입을 변경할 때는 Data, Realtime 팀원에게 알려야 합니다.
- `Cargo.toml` 의존성 추가 요청이 오면 버전을 확인하고 추가하세요.
- `main.rs`의 파이프라인 조립 흐름(cmd_record, cmd_watch)을 항상 최신 상태로 유지하세요.

---

## Data 역할

### 소유 모듈
`src/persist/`, `src/diff/`

### 주요 구현 대상

#### SqliteRepository CRUD
```
src/persist/sqlite.rs의 BuildRepository trait 메서드들을 구현해라.

요구사항:
- self.conn은 Mutex<Connection>이므로 let conn = self.conn.lock().await; 로 접근
- begin_build: builds 테이블에 INSERT, last_insert_rowid()로 BuildId 반환
- record_compilation: crate_compilations 테이블에 INSERT
- finalize_build: builds 테이블 UPDATE (finished_at, success, total_duration_ms)
- list_builds: SELECT ... ORDER BY started_at DESC LIMIT ?
- fetch_build: builds JOIN crate_compilations
- fetch_baseline: AVG, 수동 STDDEV 계산 (SQLite에 STDDEV 없음)
- duration은 밀리초(i64)로 저장, Duration::from_millis로 변환
```

#### run_persister()
```
src/persist/mod.rs의 run_persister() 함수를 구현해라.

요구사항:
- while let Some(event) = rx.recv().await 루프
- BuildStarted → repo.begin_build() → BuildId 저장
- CompilationFinished → repo.record_compilation()
- BuildFinished → repo.finalize_build()
- 나머지 이벤트(CompilationStarted, CompilerMessage)는 무시
- 첫 이벤트가 BuildStarted가 아니면 에러 반환
- 루프 종료 후 BuildId 반환
```

#### compute_diff()
```
src/diff/mod.rs의 compute_diff() 함수를 구현해라.

요구사항:
- repo.fetch_build(before)와 repo.fetch_build(after)로 두 빌드 조회
- 둘 중 하나라도 None이면 anyhow::bail!
- crate_name 기준으로 매칭:
  - before에만 있으면 CrateChange::Removed
  - after에만 있으면 CrateChange::Added
  - 둘 다 있으면 duration 비교 → Changed 또는 Unchanged (1% 이내 차이는 Unchanged)
- DurationChange의 pct_delta = (after - before) / before * 100
- critical_path는 critical_path::compute_critical_path() 사용
- crate_changes를 abs_delta_ms 내림차순 정렬
```

#### 주의사항
- `rusqlite::Connection`은 `Sync`이 아니므로 `Mutex`로 감싸져 있습니다. `lock().await` 필수.
- SQLite에 `STDDEV` 함수가 없으므로 baseline 계산 시 수동으로 구현해야 합니다.
- 테스트에서는 항상 `tempfile::TempDir`로 임시 DB를 만드세요.

---

## Realtime 역할

### 소유 모듈
`src/broker/`, `src/anomaly/`, `src/tui/`

### 주요 구현 대상

#### EventBroker::publish_loop()
```
src/broker/mod.rs의 publish_loop() 메서드를 구현해라.

요구사항:
- tokio::select! 으로 cancel 토큰과 rx.recv()를 동시 대기
- 이벤트를 받으면 모든 subscriber에 event.clone()을 send
- send 실패(채널 닫힘)한 subscriber는 Vec에서 제거
- cancel 시 즉시 종료
- rx 닫히면 루프 종료
- self을 move로 소비 (pub async fn publish_loop(self, ...))
```

#### TUI 구현
```
src/tui/ 모듈을 구현해라.

state.rs:
- TuiState struct: active_compilations (HashMap<CrateId, ActiveCompilation>),
  completed (VecDeque, 최근 N개), build_info, progress, system_stats
- ActiveCompilation: crate_id, started_at: Instant, verdict: AnomalyVerdict

render.rs:
- draw() 함수: Frame과 TuiState를 받아 ratatui 위젯으로 렌더링
- Layout: 상단(빌드 정보 + 진행률), 중앙(활성 컴파일 + 이상 감지), 하단(시스템 모니터)

system_monitor.rs:
- SystemStats struct: cpu_usage, memory_used, memory_total
- 주기적 수집 (1초 간격, sysinfo 사용)

mod.rs의 run_tui():
- crossterm으로 raw mode + alternate screen 진입
- 이벤트 루프: 100ms 간격으로 렌더링 + 이벤트 수신 + 키 입력 처리
- q 또는 Ctrl-C로 종료
- Drop 시 반드시 터미널 복원 (disable_raw_mode + LeaveAlternateScreen)
- panic hook을 설치해서 panic 시에도 터미널 복원
- 각 CompilationFinished에 대해 repo.fetch_baseline()으로 baseline 조회 후 classify()
```

#### 주의사항
- `anomaly::classify()`와 `classify_in_progress()`는 이미 구현 + 테스트됨. 그대로 사용하세요.
- TUI에서 `persist::BuildRepository` trait만 import하세요. `SqliteRepository`를 직접 import하지 마세요.
- `persist/`, `diff/` 모듈을 import하지 마세요. `BuildRepository`는 `main.rs`에서 주입받습니다.
- terminal raw mode 복원 실패는 사용자 터미널을 망가뜨립니다. 반드시 cleanup을 보장하세요.

---

## AI 협업 팁

### 효과적인 프롬프트 작성법

1. **컨텍스트를 충분히 제공하세요**
   - "persist 모듈의 begin_build 구현해줘" (X)
   - "나는 Data 담당이다. src/persist/sqlite.rs의 begin_build()를 구현해라. builds 테이블에 INSERT하고 BuildId를 반환해야 한다. 스키마는 src/persist/migrations.rs를 참고해라." (O)

2. **하나의 함수/기능 단위로 요청하세요**
   - 한 번에 모듈 전체를 구현하는 것보다, 함수 하나씩 구현 → 테스트 → 다음 함수 순서가 안전합니다.

3. **기존 코드를 참조하라고 지시하세요**
   - "src/persist/migrations.rs의 스키마를 보고 쿼리를 작성해라"
   - "src/anomaly/mod.rs의 테스트 패턴을 참고해서 테스트를 작성해라"

4. **검증을 요청하세요**
   - "구현 후 cargo clippy -- -D warnings && cargo test를 실행해서 통과하는지 확인해라"

### 코드 리뷰에 AI 활용

```
이 PR의 변경 사항을 리뷰해줘. 다음을 확인해라:
1. 의존성 방향 규칙 위반이 없는지 (model/ 역참조, Data↔Realtime 교차 참조)
2. 모든 public API에 doc comment가 있는지
3. unwrap()/expect()가 프로덕션 코드에 없는지
4. 에러 처리가 anyhow::Result를 사용하는지
5. 채널 용량이 1024인지
```

### 트러블슈팅

```
cargo clippy가 [에러 메시지]를 보여준다.
이 에러를 수정해줘. 단, 내 소유 모듈([모듈 목록])만 수정하고
다른 모듈은 절대 건드리지 마라.
```

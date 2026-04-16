# Onboarding — Day 1 체크리스트

스켈레톤을 clone한 뒤, 자기 역할의 체크리스트를 따라 Day 1 작업을 시작하세요.

## 공통 (모든 역할)

- [ ] `cargo check` 통과 확인
- [ ] `cargo test` 통과 확인 (anomaly 테스트)
- [ ] `docs/DESIGN.md` 읽기 — 전체 아키텍처, 데이터 모델, 병렬성 구조 이해
- [ ] 자기 소유 모듈의 `todo!()` 위치 파악
- [ ] feature 브랜치 생성: `feat/<role>/day1`

---

## Integrator (공용 + 이벤트 생산자)

### 소유 파일
`src/model/`, `src/cli/`, `src/supervisor/`, `src/parser/`, `src/main.rs`, `Cargo.toml`

### Day 1 작업

- [ ] **model 타입 검토 및 확정**
  - `src/model/events.rs`의 `BuildEvent` enum이 Cargo JSON 출력과 매핑되는지 확인
  - 필요하면 필드 추가/수정 (이 때 팀에 공유)
  - `Display` trait 구현 추가 (로깅용)

- [ ] **supervisor 구현**
  - `src/supervisor/mod.rs`의 `spawn_build()` 함수 구현
  - `tokio::process::Command`로 `cargo build --message-format=json-render-diagnostics` 실행
  - stdout을 `tokio::io::BufReader`로 감싸 라인 단위 읽기
  - 각 라인을 `mpsc::Sender<String>`으로 전송
  - `SupervisorHandle`로 프로세스 종료 대기/취소

- [ ] **테스트 픽스처 생성**
  - 아무 Rust 프로젝트에서 `cargo build --message-format=json-render-diagnostics 2>/dev/null > tests/fixtures/sample_output.jsonl` 실행
  - 이 파일을 커밋에 포함

- [ ] **통합 확인**
  - `cargo run -- record -- --help` 가 clap 도움말을 보여주는지 확인

### 의존하지 않는 것
- Data/Realtime 모듈 — Day 1에는 supervisor만 구현하면 됨

### 다른 팀원에게 제공하는 것
- `model/` 타입이 확정되면 Slack/PR으로 공유
- `supervisor::spawn_build()`가 동작하면 Parser 테스트에 활용 가능

---

## Data 담당

### 소유 파일
`src/persist/`, `src/diff/`

### Day 1 작업

- [ ] **DB 스키마 확정**
  - `src/persist/migrations.rs`의 SQL 확인 및 필요 시 수정
  - 인덱스 전략 검토

- [ ] **SqliteRepository::open() 테스트**
  - `cargo test`로 DB 생성/마이그레이션 확인
  - `tempfile`로 임시 DB 만들어 테스트

- [ ] **begin_build() 구현**
  - `BuildStarted` 이벤트를 받아 `builds` 테이블에 INSERT
  - `BuildId` 반환

- [ ] **record_compilation() 구현**
  - `CompilationFinished` 이벤트를 `crate_compilations` 테이블에 INSERT

- [ ] **finalize_build() 구현**
  - `BuildFinished` 이벤트로 `builds` 레코드 UPDATE (finished_at, success, total_duration_ms)

- [ ] **run_persister() 구현**
  - `mpsc::Receiver<BuildEvent>`를 loop로 소비
  - 이벤트 종류별 분기 (begin_build, record_compilation, finalize_build)

### 의존하지 않는 것
- Supervisor, Parser — `BuildEvent`를 직접 만들어 테스트 가능
- Broker, TUI — Data 모듈과 무관

### 다른 팀원에게 제공하는 것
- `BuildRepository` trait가 구현되면 Realtime 팀이 TUI에서 baseline 조회 가능
- `run_persister()`가 동작하면 Integrator가 record 명령 조립 가능

---

## Realtime 담당

### 소유 파일
`src/broker/`, `src/anomaly/`, `src/tui/`

### Day 1 작업

- [ ] **ratatui 맛보기**
  - `cargo run --example ratatui_hello` 실행
  - 키 입력으로 종료되는지 확인
  - ratatui의 `Frame`, `Widget`, `Layout` 개념 파악

- [ ] **broker 구현**
  - `src/broker/mod.rs`의 `EventBroker` 구현
  - `subscribe()`: 새 `mpsc::channel` 생성, sender를 내부 Vec에 보관, receiver 반환
  - `publish_loop()`: 입력 채널에서 이벤트를 받아 모든 subscriber에 send
  - 닫힌 subscriber는 자동 제거

- [ ] **anomaly 테스트 확인**
  - `cargo test` — 이미 작성된 4개 테스트가 통과하는지 확인
  - 추가 edge case 테스트 작성 (std_dev가 0인 경우, sample_count가 1인 경우 등)

- [ ] **TUI state 모델 설계**
  - `src/tui/state.rs`에 `TuiState` struct 설계
  - 어떤 데이터를 화면에 표시할지 정리:
    - 현재 컴파일 중인 crate 목록 + 경과 시간
    - 완료된 crate 목록 (최근 N개)
    - 전체 진행률 (완료 crate 수 / 예상 전체 수)
    - CPU/메모리 사용량
    - 각 crate의 AnomalyVerdict

### 의존하지 않는 것
- Supervisor, Parser — BuildEvent를 직접 만들어 broker 테스트 가능
- SQLite — anomaly는 순수 함수, TUI는 Day 1에는 mock 데이터로 개발

### 다른 팀원에게 제공하는 것
- `EventBroker`가 동작하면 Integrator가 watch 명령 조립 가능
- `anomaly::classify()`는 이미 동작 — TUI에서 바로 사용 가능

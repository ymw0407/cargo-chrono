# cargo-chronoscope 최종 보고서

> 작성: 2026-05-06 · `v0.1.9` 시점 기준

---

## 1. 프로젝트 개요

`cargo-chronoscope`은 **Cargo 빌드 성능 옵저버**입니다. 일반 Rust 개발자가 자기 프로젝트의 `cargo build` 시간이 PR마다 어떻게 변하는지를 *측정·기록·비교*할 수 있게 하는 CLI + GitHub Action 도구입니다.

### 1.1 핵심 가치 제안

`cargo build --message-format=json-render-diagnostics`가 내보내는 머신 판독 가능한 빌드 이벤트 스트림을 소비해 모든 빌드를 로컬 SQLite 데이터베이스에 기록하고, 네 가지 방식으로 분석합니다.

| 서브커맨드 | 역할 |
|---|---|
| `record` | 한 번의 `cargo build`를 기록 |
| `watch`  | 빌드 중 ratatui 기반 실시간 TUI 대시보드 |
| `ls`     | 최근 빌드 목록 |
| `diff <a> <b>` | 두 빌드 간 per-crate 변동량 + critical path 비교 |

GitHub Action으로 래핑되어, 사용자 레포의 PR마다 빌드 시간 diff가 sticky 코멘트로 자동 게시됩니다 (포크 PR 포함).

### 1.2 포지셔닝

Rust 생태계에서 이 정확한 niche를 메우는 도구는 사실상 비어있습니다.

- `cargo --timings` (공식 내장): 단일 빌드 분석. 우리는 빌드 *간* 비교에 특화.
- `sccache`: 컴파일러 캐시(가속). 우리는 측정. 보완 관계.
- `cargo-criterion`: 같은 "track over time" 패턴이지만 런타임 벤치마크용.
- Rust 공식 [2025 H2 goal "Prototype Cargo build analysis"][rust-goal]의 "외부 도구로 historical trends 분석" 트랙에 정확히 해당합니다.

[rust-goal]: https://rust-lang.github.io/rust-project-goals/2025h2/cargo-build-analysis.html

### 1.3 릴리스 이력

`v0.1.0` (2026-05-03) → `v0.1.9` (2026-05-05). 약 3일간 9번의 minor patch release.

전 release는 [crates.io](https://crates.io/crates/cargo-chronoscope) 와 [GitHub Marketplace](https://github.com/marketplace/actions/cargo-chronoscope) 양쪽에 publish됨. 4개 플랫폼 prebuilt 바이너리 (Linux x86_64, macOS Intel, macOS Apple Silicon, Windows x86_64).

---

## 2. 팀 구성과 역할 분담

3인 팀으로 시작했고, 각자 전담 영역을 가졌습니다. 프로젝트 후반부에 외부 컨트리뷰션이 들어오면서 이 분담은 "작업 시작점"으로 완화되었지만, 초기 아키텍처는 이 분담을 따라 형성되었습니다.

| 역할 | GitHub | 이름 | 담당 모듈 |
|---|---|---|---|
| **Integrator** | [@ymw0407](https://github.com/ymw0407) | 윤민우 | `src/model/`, `src/cli/`, `src/supervisor/`, `src/parser/`, `src/main.rs`, `Cargo.toml` |
| **Data**       | [@yangfeiran20252335](https://github.com/yangfeiran20252335) | 양비연 | `src/persist/`, `src/diff/` |
| **Realtime**   | [@addbum421](https://github.com/addbum421) | 유범익 | `src/broker/`, `src/anomaly/`, `src/tui/` |

### 2.1 누가 무엇을 했나

**윤민우 (Integrator, @ymw0407)**

전반 통합 + 인프라 + 외부 노출:
- 모든 모듈 간 인터페이스 정의 (`model/`의 `BuildEvent`, `BuildId`, `BuildProfile` 등)
- `cli/` 의 clap 기반 서브커맨드 정의 + `src/main.rs` 의 4-task 파이프라인 어셈블리
- `supervisor/` — `cargo build` 자식 프로세스를 spawn하고 stdout(JSON) + stderr("Compiling foo") 라인을 단일 채널로 머지
- `parser/` — JSON 이벤트를 `BuildEvent` 스트림으로 변환
- **GitHub Action 발행 인프라**: `action.yml` 작성, release 워크플로 (4 플랫폼 matrix), `cargo-binstall` 메타데이터, `action-v1` moving tag 컨벤션 + 후일 `vX.Y.Z` single-namespace로 통합
- **CI 파이프라인**: `Build performance` 워크플로의 `workflow_run` 컴패니언 패턴 도입 (포크 PR 스티키 코멘트 지원)
- 28개 PR 머지, release 작업 v0.1.0 ~ v0.1.9 전담
- 외부 컨트리뷰션 리뷰 / 머지 / 응답

**양비연 (Data, @yangfeiran20252335)**

영속성 레이어 + 분석 도메인:
- `src/persist/sqlite.rs` — `BuildRepository` 트레잇의 SQLite 구현
  - WAL 저널 모드 + 5초 `busy_timeout`
  - `SqliteRepository` 구조체 + `tokio::sync::Mutex<Connection>` 래핑 (rusqlite의 `Connection`이 `Sync`가 아닌 점 회피)
  - `begin_build` / `record_compilation` / `finalize_build` / `delete_build` / `list_builds` / `fetch_build` / `fetch_baseline`
- `src/persist/migrations.rs` — `IMMEDIATE` 트랜잭션 + `PRAGMA user_version` 기반의 idempotent atomic 마이그레이션
- `src/diff/` — 두 빌드 간 per-crate diff + critical-path 추출 알고리즘
- 베이스라인 통계 (mean ± 2σ) 쿼리

**유범익 (Realtime, @addbum421)**

실시간 / 사용자 인터페이스:
- `src/tui/` — ratatui 0.30 기반 대시보드
  - 활성 컴파일별 elapsed time 카운터
  - Recently finished 리스트 with anomaly 마커 (`▲ slower` / `▼ faster` / `· normal`)
  - CPU + memory 시스템 메트릭 (`tui::system_monitor`)
  - 60fps 렌더 tick + raw mode + alternate screen 관리, panic-safe 복원 (RAII guard + panic hook)
- `src/broker/` — fan-out 이벤트 broadcaster (Persister + TUI 두 subscriber)
- `src/anomaly/` — 베이스라인 통계 기반 분류기 (`classify`, `classify_in_progress`)
- 후반부 두 개의 critical bug fix:
  - **PR [#77](https://github.com/ymw0407/cargo-chronoscope/pull/77)** — `SupervisorHandle.cancel()` wire-up: `cmd_record`/`cmd_watch`가 handle을 버려서 Ctrl-C가 cargo 자식 프로세스를 못 죽이던 문제 해결
  - **PR [#79](https://github.com/ymw0407/cargo-chronoscope/pull/79)** — `fetch_baseline`에 `JOIN builds ON success = 1` 추가: 실패 빌드의 컴파일 샘플이 anomaly 베이스라인을 오염시키던 silent-correctness 버그 해결

### 2.2 작업량 통계

| 지표 | 수치 |
|---|---|
| 총 머지된 PR | 43개 |
| 총 등록된 이슈 | 31개 (open + closed 합산) |
| 내부 팀 PR (3인 합산) | 33개 (윤민우 28 + 유범익 4 + 양비연 1) |
| 외부 컨트리뷰션 PR | 4개 (txhno 2, q404365631 1, fatima836 1) |
| Dependabot 자동 PR | 6개 |
| Release 횟수 | 9회 (`v0.1.0` ~ `v0.1.9`) |

---

## 3. 핵심 race condition 3종과 해결

빌드 이벤트 파이프라인은 본질적으로 다중 비동기 컴포넌트가 채널과 공유 상태로 협력하는 구조라, race condition이 곳곳에 잠재합니다. 그중 실제로 만나서 해결한 세 가지를 정리합니다.

설계 문서 원본은 [`CONCURRENCY.md`](CONCURRENCY.md)에 있고, 이 보고서에서는 *문제 → 해결 → 검증* 순서로 압축합니다.

### 3.1 Slow subscriber → backpressure → 빌드 기록 멈춤 (R4)

**문제.** `watch` 모드의 파이프라인은 `Supervisor → Parser → Broker → (Persister + TUI)`. Tokio의 `mpsc`는 기본적으로 backpressure를 제공하므로, **TUI가 잠깐 느려져 자기 채널을 비우지 못하면** 그 backpressure가 Broker → Parser → Supervisor 까지 거슬러 올라가 결국 cargo의 stdout 읽기가 멈춥니다. 즉 *빌드 기록 자체가* 정지합니다.

화면 떨림 (시각적 결손) 정도의 비용으로 막아도 될 일이 데이터 결손 (빌드 기록 멈춤) 으로 번지는 게 핵심 위험.

**해결.** Broker가 subscriber별로 *독립된 bounded mpsc*를 들고, fan-out 시 `try_send`를 사용해 **가득 찬 subscriber에 대해서만 그 이벤트를 drop**합니다. 빌드 기록 우선·시각 결손 후순위.

```rust
// src/broker/mod.rs
pub fn subscribe(&mut self, buffer: usize) -> mpsc::Receiver<BuildEvent> {
    let (tx, rx) = mpsc::channel(buffer);   // ← subscriber별 독립 채널
    self.subscribers.push(tx);
    rx
}

pub async fn publish_loop(
    mut self,
    mut rx: mpsc::Receiver<BuildEvent>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(()),
            maybe_event = rx.recv() => {
                let event = match maybe_event { Some(e) => e, None => return Ok(()) };
                // Fan-out: try_send avoids blocking on slow/full subscribers.
                self.subscribers.retain(|tx| match tx.try_send(event.clone()) {
                    Ok(())                                       => true,
                    Err(mpsc::error::TrySendError::Full(_))      => true,  // drop the event, keep subscriber
                    Err(mpsc::error::TrySendError::Closed(_))    => false, // remove subscriber
                });
            }
        }
    }
}
```

```rust
// src/main.rs (cmd_watch 호출부)
let persister_rx = event_broker.subscribe(1024);  // 절대 drop되면 안 됨
let tui_rx       = event_broker.subscribe(1024);  // 가득 차면 drop OK
```

설계상 1024 버퍼는 일반 빌드 (수백 이벤트 미만)에서는 가득 차지 않습니다. 즉 *통상 경로에서는 drop이 발생하지 않고*, 매우 큰 워크스페이스나 느린 머신에서의 *방어선*으로 동작합니다. 데이터 결손이 절대 발생하지 않는다는 보장과, 시각 결손이 가능하지만 자동 회복된다는 보장을 얻습니다.

**검증.** `src/broker/mod.rs` 의 `#[cfg(test)] mod tests`에 5개의 contract 테스트가 있습니다:
- `broadcasts_events_to_all_subscribers` — 정상 fan-out
- `cancel_terminates_publish_loop` — cancel 토큰이 루프 종료시킴
- `closed_input_terminates_publish_loop` — 입력 채널 close 시 정상 종료
- `dropped_subscriber_is_removed` — Closed 분기 검증
- `slow_subscriber_does_not_block_others` — Full→drop 분기 검증

특히 마지막 테스트는 buffer를 1로 줄인 인위적 환경에서 한 subscriber를 drain하지 않은 채 다른 subscriber로 이벤트가 정상 흐르는 것을 확인합니다 — Full→drop 분기의 직접 reproducer.

### 3.2 Cancel 오염 → 베이스라인 망가짐 (R5)

**문제.** 빌드 도중 사용자가 Ctrl-C를 누르거나 TUI에서 `q`를 누르면, 이미 `record_compilation`을 통해 일부 crate에 대한 row가 DB에 들어가 있을 수 있습니다. 이 partial 데이터를 그대로 두면:

1. `cargo-chronoscope ls` 출력이 미완 빌드로 더러워짐
2. `fetch_baseline`이 미완 row를 mean/stddev에 포함 → 다음 anomaly 판정이 *조용히* 부정확해짐 (silent-correctness 버그)

**해결.** 두 단계로 분리:

(a) Cancel 감지 → 미완 build의 row 통째 삭제 (`finalize_or_discard`):

```rust
// src/main.rs
async fn finalize_or_discard(
    repo: Arc<dyn BuildRepository>,
    build_id: BuildId,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    if cancel.is_cancelled() {
        repo.delete_build(build_id).await?;          // ← 미완 row 통째 삭제
        eprintln!("Build interrupted — not recorded.");
    } else {
        println!("Build {} recorded.", build_id);
    }
    Ok(())
}
```

(b) 삭제는 단일 트랜잭션 안에서 atomic하게:

```rust
// src/persist/sqlite.rs
async fn delete_build(&self, id: BuildId) -> anyhow::Result<()> {
    let mut conn = self.conn.lock().await;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM crate_compilations WHERE build_id = ?1", [id.0])?;
    tx.execute("DELETE FROM builds WHERE id = ?1", [id.0])?;
    tx.commit()?;                                     // ← 둘 중 하나만 지워지는 경우 없음
    Ok(())
}
```

**중요한 결정**: cancel과 *cargo의 자체 실패*를 구분합니다. 컴파일 에러로 cargo가 비정상 종료한 경우는 row를 *유지*하고 `success = 0`으로만 마킹합니다 — 사용자가 사후에 디버깅할 가치가 있는 데이터.

**후일 발견된 보완**. 처음 설계는 위 두 단계로 충분했지만, 이후 **PR [#79](https://github.com/ymw0407/cargo-chronoscope/pull/79)** (유범익 작성) 에서 `fetch_baseline`이 `success = 0` 행도 베이스라인 평균에 포함시키던 누락을 발견했습니다. SQL을 다음과 같이 수정:

```sql
SELECT COUNT(*), AVG(cc.duration_ms), MIN(cc.duration_ms), MAX(cc.duration_ms),
       AVG(cc.duration_ms * cc.duration_ms)
FROM crate_compilations cc
JOIN builds b ON cc.build_id = b.id
WHERE cc.crate_name = ?1 AND b.success = 1
```

`b.success = 1` 필터로 실패·미완 빌드의 컴파일 샘플을 모두 베이스라인에서 배제. 이 수정은 v0.1.8에서 release되어 cargo-chronoscope을 CI에 끼워둔 모든 외부 사용자에게 silent-correctness 회복을 가져왔습니다.

**검증.** `src/persist/sqlite.rs` 테스트:
- `delete_build_removes_build_and_compilations` — 트랜잭션 한 번에 양쪽 테이블 삭제됨
- `delete_build_is_idempotent_on_missing_id` — 존재하지 않는 build_id로 호출해도 panic하지 않음
- `fetch_baseline_excludes_compilations_from_failed_builds` — 같은 crate에 대해 successful 빌드(100ms)와 failed 빌드(9999ms)를 각각 기록 후, `fetch_baseline`이 sample_count=1, mean=100ms를 반환하는지 (즉 9999ms가 배제되는지) 검증
- `fetch_baseline_computes_mean_min_max` — 정상 케이스 회귀 테스트

마지막 테스트는 PR #79에서 함께 들어온 회귀 테스트로, 만약 누가 향후 SQL의 `JOIN builds`를 실수로 빼면 즉시 빨갛게 떨어집니다.

### 3.3 SQLite 동시 접근 → schema race (R3)

**문제.** 같은 프로젝트 디렉터리에서 두 명의 사용자가 동시에 `cargo-chronoscope record`를 실행하거나, 한 명이 다른 터미널에서 `cargo-chronoscope ls`를 호출하면, 같은 `.cargo-chronoscope/history.db` 파일을 두 프로세스가 동시에 엽니다.

문제는 *마이그레이션 race*: 두 프로세스가 동시에 schema check를 하고, 둘 다 "schema가 없음" → 둘 다 `CREATE TABLE` 실행 → 한쪽은 `SQLITE_BUSY` 또는 부분 적용으로 실패.

**해결.** 세 가지 방어선:

(a) `Connection::open()` 시점에 5초 `busy_timeout`을 걸어, writer-lock 충돌 시 즉시 실패 대신 자동 재시도:

```rust
// src/persist/sqlite.rs::open
conn.busy_timeout(Duration::from_secs(5))?;
```

(b) WAL 저널 모드 활성화로 reader가 writer를 막지 않게:

```rust
conn.pragma_update(None, "journal_mode", "wal")?;
```

(c) 마이그레이션 자체를 `BEGIN IMMEDIATE` 트랜잭션 안에 넣고, 트랜잭션 *내부*에서 `PRAGMA user_version`을 확인해 idempotent하게:

```rust
// src/persist/migrations.rs
const SCHEMA_VERSION: i32 = 1;

pub(crate) fn run_migrations(conn: &mut Connection) -> anyhow::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // 트랜잭션 안에서 버전 체크 → 다른 프로세스가 이미 했으면 no-op
    let current: i32 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current >= SCHEMA_VERSION {
        return Ok(());
    }

    tx.execute_batch(SCHEMA_V1)?;
    tx.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION))?;
    tx.commit()?;
    Ok(())
}
```

`IMMEDIATE` 트랜잭션은 writer lock을 즉시 획득하므로 두 프로세스가 동시에 `open()`을 호출해도 SQLite가 *직렬화*시킵니다. 두 번째 프로세스는 첫 번째의 트랜잭션이 끝난 뒤에야 `SELECT user_version`을 보고, 이미 마이그레이션됐음을 알아차린 뒤 그냥 리턴합니다. **schema가 두 번 적용되거나 일부만 적용되는 일이 없습니다.**

**검증.** `src/persist/sqlite.rs` 의 race 테스트:

```rust
#[tokio::test]
async fn concurrent_open_and_write_from_two_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");

    // 두 task가 같은 파일을 동시에 open하고 쓰기 시도
    let (a, b) = tokio::join!(
        async { /* open + begin_build + record_compilation + finalize_build */ },
        async { /* same */ },
    );
    a.unwrap(); b.unwrap();

    // 양쪽 build 모두 정상 기록됐는지 확인
    let repo = SqliteRepository::open(&db).await.unwrap();
    let builds = repo.list_builds(10).await.unwrap();
    assert_eq!(builds.len(), 2);
}
```

Tokio의 두 task에서 `tempfile::TempDir`로 격리된 동일 DB 파일을 동시에 열고 각각 빌드 한 번씩 기록한 뒤, 양쪽 모두 정상으로 들어갔는지 확인합니다. busy_timeout · WAL · IMMEDIATE 트랜잭션이 함께 동작하지 않으면 한쪽이 `SQLITE_BUSY`로 실패하므로 회귀 시 즉시 빨갛게 됩니다.

### 3.4 발견 후 수정된 또 하나의 race — Ctrl-C 가 cargo를 못 죽이던 문제 (R7)

위 3종은 설계 단계부터 인지되어 있었지만, 한 가지 race는 후반부에 발견되어 PR로 수정됐습니다:

**문제.** `cmd_record`/`cmd_watch`가 `supervisor::spawn_build`가 반환하는 `SupervisorHandle`을 `let (line_rx, _handle) = ...` 로 *버리고* 있었습니다. 그 결과 사용자가 Ctrl-C 또는 TUI `q`를 눌러 `CancellationToken`이 fire되어도, supervisor에게 신호가 *전달되지 않아* cargo 자식 프로세스가 백그라운드에서 끝까지 컴파일을 계속했습니다. UI상 "interrupted"라고 표시되는데 실제로는 CPU가 계속 돌아가는 상태.

**해결 (PR [#77](https://github.com/ymw0407/cargo-chronoscope/pull/77), 유범익 작성).** handle을 spawn한 작은 task가 cancel 토큰을 await하다가 fire되면 `handle.cancel()`을 호출:

```rust
// src/main.rs (cmd_record / cmd_watch)
let (line_rx, handle) = supervisor::spawn_build(cargo_args, workspace_dir).await?;

let cancel_for_supervisor = cancel.clone();
tokio::spawn(async move {
    cancel_for_supervisor.cancelled().await;
    handle.cancel();   // ← cargo 자식 프로세스 즉시 죽임
});
```

**검증.** `src/supervisor/mod.rs` 에 두 개의 짝지은 회귀 테스트:
- `issue_60_outer_cancel_without_wiring_does_not_kill_child` — 버그 그대로 재현 (handle을 버린 채 cancel 토큰 fire → child가 1초 안에 안 죽음을 검증). 미래의 누가 wire-up을 다시 풀어버리면 이 테스트가 빨갛게 됩니다.
- `wired_cancel_kills_child_on_outer_cancel` — fix가 적용된 상태에서 cancel 토큰 fire → 1초 안에 child가 죽음을 검증.

negative test 형태로 *버그가 어떻게 생겼는지를 코드로 문서화*했다는 점이 흥미롭습니다.

---

## 4. 안전성 검증 — 전반적 테스트 전략

각 race에 대한 직접 reproducer 외에도, 프로젝트 전체에 걸쳐 다음 검증 레이어를 운영했습니다.

### 4.1 테스트 매트릭스

| 종류 | 위치 | 개수 (대략) | 도구 |
|---|---|---|---|
| 단위 테스트 | 각 모듈 파일의 `#[cfg(test)] mod tests {}` | ~125 | `cargo test` |
| 통합 테스트 | `tests/` 루트 | ~10 | `cargo test --tests` |
| 동시성 race 테스트 | `broker/`, `persist/`, `supervisor/` | 8 | `tokio::join!`, `tempfile::TempDir` |
| 회귀 테스트 (negative) | `supervisor/`, `tui/` | 2 | "버그를 코드로 문서화" |
| Action E2E | self-CI on every PR | 1 워크플로 | Build performance 워크플로가 자기 자신을 측정 |
| **합계** | | **~138** | |

`cargo test` 한 번에 138개 테스트가 돌고, Linux/macOS에서는 모두 통과합니다. (Windows에서는 5개 supervisor 테스트가 `sh -c` / `/tmp` 의존성 때문에 환경적으로 실패하며, 이는 별도 이슈 [#57](https://github.com/ymw0407/cargo-chronoscope/issues/57)로 트래킹.)

### 4.2 사보타주 검증 (sabotage testing)

회귀 테스트가 *진짜로* 회귀를 잡는지 확인하기 위한 메타-검증 기법:

1. 회귀 테스트가 통과하는 상태에서 시작
2. fix를 의도적으로 되돌려 (sabotage) 버그를 재도입
3. 테스트가 정확히 떨어지는지 확인
4. 떨어지면 → 회귀 테스트는 진짜로 그 회귀를 잡음
5. 떨어지지 않으면 → 회귀 테스트가 무의미

예: PR [#55](https://github.com/ymw0407/cargo-chronoscope/pull/55) 리뷰 시 외부 컨트리뷰터의 첫 번째 회귀 테스트가 통과는 하지만 정작 원래 버그 패턴 (`wait_for_exit_key` 내부에 `cancel.cancel()` 재도입) 을 사보타주했을 때 *그대로 통과*하는 것을 발견했습니다. 즉 테스트는 형식상 회귀를 막는 것처럼 보이지만 실제로는 잡지 못하는 상태였고, 이를 작성자에게 코멘트로 알리고 두 번째 iteration에서 진짜로 잡는 형태로 재작성됐습니다.

### 4.3 외부 사용자 검증 — 자기 자신을 측정하는 CI

`Build performance` 워크플로는 cargo-chronoscope이 자기 자신의 `cargo build`를 매 PR마다 측정하고 sticky 코멘트를 게시하도록 셋업되어 있습니다. 이 dogfooding 루프는:

- 실제 production 사용 시나리오를 매 PR에서 행사
- per-crate 회귀가 main branch로 스며들지 못하게 방어
- workflow_run 컴패니언 패턴 (포크 PR 지원) 의 동작을 매 외부 PR마다 검증

PR #61 (dead-code 리팩터링) 시 **빌드 시간이 -5.4% 빨라진 것이 sticky 코멘트로 자동 검출**되어 코멘트로 박힌 사례가 도구의 self-validation으로서 가장 인상적입니다.

### 4.4 Clippy + fmt를 hard gate로

CI 파이프라인은 `cargo clippy --all-targets -- -D warnings`를 *경고도 에러로* 처리합니다. 이는 dead_code lint를 경계로 사용해 "쓰지 않는 코드"가 조용히 누적되는 것을 방지합니다. 실제로 이 정책이 PR [#61](https://github.com/ymw0407/cargo-chronoscope/pull/61)을 견인했습니다 — `#![allow(dead_code)]` 한 줄을 제거하니 10개의 미사용 항목이 노출되어 일괄 정리.

`cargo fmt --check` 는 별개 요구로 적용되어 모든 머지된 코드가 표준 포맷을 따르도록 강제.

---

## 5. 성과

### 5.1 코드 품질 지표

| 지표 | 값 |
|---|---|
| 총 Rust 코드 줄 수 | ~3,500 (테스트 포함) |
| 테스트 / 비테스트 비율 | 약 1:2 |
| `clippy --all-targets -- -D warnings` | 0 warnings |
| `#![allow(dead_code)]` 잔존 | 1개 (`model/persisted.rs`, v1.0 로드맵 forward-compat 슬롯, 트래킹된 issue 4개에 직접 매핑) |
| Cargo dependency 수 | 11 직접 + ~240 transitive |
| Race condition 회귀 테스트 | 8개 (broker 5 + persist 2 + supervisor 2 + tui 1 wait_for_exit_key) |

### 5.2 외부 노출

- **crates.io**: `cargo-chronoscope 0.1.9` published. Cumulative downloads: 80+ (대부분 우리 자신과 봇)
- **GitHub Marketplace**: [cargo-chronoscope action](https://github.com/marketplace/actions/cargo-chronoscope) 게시. 4개 플랫폼 prebuilt 바이너리 (Linux x86_64, macOS Intel, macOS Apple Silicon, Windows x86_64)
- **TWiR (This Week in Rust)**: PR [#8022](https://github.com/rust-lang/this-week-in-rust/pull/8022) 제출. 2026-05-06 발행분 "Project/Tooling Updates" 섹션에 entry 등록 진행 중
- **GitHub Stars**: 1 (홍보 직전 시점)
- **외부 컨트리뷰션**: 4개 PR 머지 (3명의 외부 컨트리뷰터로부터)

### 5.3 외부 컨트리뷰션 사례

오픈소스 가치는 결국 *얼마나 많은 외부인이 가져다 쓰고 기여하는가*로 측정됩니다. 짧은 기간이지만 의미 있는 패턴이 형성되었습니다:

| 컨트리뷰터 | 머지된 PR | 종류 |
|---|---|---|
| `txhno` (Roshan Warrier) | #54 (액션 default version), #55 (TUI 회귀 테스트) | feature + test |
| `q404365631` | #53 (`.gitignore` typo) | typo fix |
| `fatima836` | #52 (README Status 섹션 갱신) | docs |

특히 PR #55는 두 번의 iteration을 거치며 사보타주 검증으로 회귀 테스트의 실효성을 입증한 사례로, **컨트리뷰터-메인테이너 협업 사이클이 단순 "코드 머지"를 넘어 품질 검증까지 함께 이루어진** 모델 케이스입니다.

### 5.4 정책 진화 — OSS-friendly 거버넌스로의 이동

프로젝트는 3인 팀의 internal collaboration 룰로 시작했지만, 외부 컨트리뷰션이 들어오기 시작한 시점에 다음과 같이 정책을 재정비했습니다:

- **Module ownership 룰을 historical context로 강등**: 원래 [`docs/internal/ROLE_OWNERSHIP.md`](ROLE_OWNERSHIP.md)는 강제 룰이었지만, OSS 환경에서는 신규 컨트리뷰터에게 진입 장벽으로 작용. 룰을 완화하고 [`.github/CODEOWNERS`](../../.github/CODEOWNERS)로 자동 리뷰 라우팅으로 전환
- **AI-assisted contribution 명시적 환영**: 메인테이너 자신이 AI 도구를 사용하므로 외부 컨트리뷰터에게 다른 잣대를 적용하지 않음. 단 페이스 (한 번에 1-2 active PR)는 review capacity를 위해 권장
- **Good first issue 풀 운영**: 5개의 GFI 이슈 ([#66-#70](https://github.com/ymw0407/cargo-chronoscope/issues?q=label%3A%22good+first+issue%22)) 등록으로 신규 컨트리뷰터 진입로 다양화
- **Tag namespace 통합 (v0.1.9)**: GitHub Marketplace의 자동 추천이 정확히 동작하도록 `vX.Y.Z`를 단일 canonical tag로 채택. dual-namespace (`vX.Y.Z` + `action-v*`) 의 사용자 혼동 제거

### 5.5 인프라 정착

- **Release 자동화**: `v*` 태그 push만으로 4 플랫폼 binary 빌드 + crates.io publish + GitHub Release archive 첨부
- **PR sticky 코멘트**: 동일 레포 PR + fork PR 모두에 빌드 시간 diff 자동 부착 (`workflow_run` 컴패니언 패턴)
- **Semantic Versioning**: pre-1.0이지만 [Keep a Changelog](https://keepachangelog.com/) 엄격히 준수. 9개 release 모두 명시적 changelog entry

---

## 6. 오픈소스 프로젝트로서의 가치

### 6.1 학술적 / 기술적 가치

- **Rust 공식 H2 goal에 외부에서 기여**: rust-lang의 [2025 H2 "Cargo build analysis"][rust-goal] 목표의 *historical trends 분석* 트랙은 정확히 cargo-chronoscope의 영역. 공식 도구가 cover하지 않는 cross-build 시계열 분석을 외부 도구로 빠르게 채움
- **race condition 처리 패턴의 교과서적 사례**: 본 보고서 §3에 정리된 세 가지 케이스는 Rust의 `tokio` + `mpsc` + `CancellationToken` + SQLite 조합에서 마주칠 수 있는 전형적 패턴. 다른 프로젝트의 참고 자료가 될 수 있음
- **`workflow_run` 패턴 데모**: GitHub Actions의 fork-PR 보안 제약 우회 방법을 깔끔히 구현. 이 패턴 자체가 다른 OSS 프로젝트에 응용 가능한 *재사용 가능한 인프라 idiom*

### 6.2 실용적 가치

- **CI에 한 줄 install로 빌드 시간 회귀 감지**: `uses: ymw0407/cargo-chronoscope@v0.1.9` 한 줄로 자기 Rust 프로젝트의 PR마다 빌드 시간 변화를 자동 측정·표시
- **로컬 실시간 모니터링**: `cargo-chronoscope watch` 한 명령으로 활성 컴파일별 elapsed time + anomaly 마커 + 시스템 메트릭을 실시간 확인. 큰 워크스페이스에서 어느 crate가 항상 느린지를 *추측이 아닌 데이터로* 알 수 있음
- **백업 데이터로서의 빌드 이력**: 프로젝트 디렉터리에 SQLite로 누적되는 빌드 데이터는 그 자체로 향후 분석 자산. 외부 서버 의존 없이 로컬 우선

### 6.3 협업 / 거버넌스 가치

- **소규모 팀의 OSS 진입 사례**: 3인 internal 팀 → 외부 컨트리뷰션 받는 OSS로 전환하면서 정책을 단계적으로 완화한 흐름은 비슷한 단계의 다른 팀에게 참고 가능
- **AI-assisted 컨트리뷰션에 대한 일관된 태도**: 메인테이너 본인이 AI를 활용하므로 외부 컨트리뷰터에게도 적대적이지 않은 정책. 다만 페이스 / 품질 / 검증은 동일 기준 적용

---

## 7. 향후 과제 — v1.0 로드맵

[Issue #51](https://github.com/ymw0407/cargo-chronoscope/issues/51) 에 v1.0 도달을 위한 작업이 정리되어 있습니다.

| Stage | 항목 | 이슈 |
|---|---|---|
| **차별화** | 풍부한 PR 코멘트 (sparkline, top movers, threshold 마커) | [#41](https://github.com/ymw0407/cargo-chronoscope/issues/41) |
| | 회귀 임계값 초과 시 워크플로 fail 모드 | [#42](https://github.com/ymw0407/cargo-chronoscope/issues/42) |
| **분석 깊이** | 설정 가능한 sigma threshold | [#44](https://github.com/ymw0407/cargo-chronoscope/issues/44) |
| | `cargo --timings` JSON ingest | [#45](https://github.com/ymw0407/cargo-chronoscope/issues/45) |
| | git ref 기반 baseline (`diff --base <ref>`) | [#46](https://github.com/ymw0407/cargo-chronoscope/issues/46) |
| | Workspace-aware per-crate baselines | [#47](https://github.com/ymw0407/cargo-chronoscope/issues/47) |
| **출력** | `render` HTML report 명령 | [#48](https://github.com/ymw0407/cargo-chronoscope/issues/48) |
| | `ls` 필터 플래그 (`--since`, `--profile`, `--branch`) | [#49](https://github.com/ymw0407/cargo-chronoscope/issues/49) |
| **방향** | depth vs breadth vs team-collab 결정 | [#50](https://github.com/ymw0407/cargo-chronoscope/issues/50) |

### 1.0 의 정의

> "I would tell a stranger to depend on this in their CI without a disclaimer."

Stage 1 polish (#35-#40, 모두 closed) + Stage 2 Action UX 차별화 (#41-#43) + 분석 깊이 일부 (#44, #46) 가 완료되면 1.0 cut. 나머지 항목은 1.x로 미룸.

---

## 8. 결론

3인 팀의 internal 프로젝트로 시작한 cargo-chronoscope은 3일 만에 9번의 release를 거쳐 외부 컨트리뷰션을 받는 오픈소스 도구로 정착했습니다. 본 보고서가 강조한 race condition 3종은 모두 *발견 → 설계 → 검증* 의 사이클을 거쳐 production 코드와 회귀 테스트로 동시에 잠겨 있고, 그중 하나 (R7, Ctrl-C wire-up) 는 후반부 자체 발견 + 팀 내 PR로 수정된 사례로 프로젝트의 자체 검증 능력을 보여주었습니다.

오픈소스 도구로서의 가치는 두 축에서 뚜렷합니다:

1. **기술적 빈틈을 메움** — Rust 생태계에서 비어있던 cross-build 빌드 성능 추적 niche
2. **인프라 패턴의 시범 사례** — fork-PR sticky 코멘트, single-namespace tag 통합, AI-friendly OSS governance 등이 다른 프로젝트가 차용 가능한 형태로 정착

v1.0까지의 경로는 명확하고 (#51), 그 사이의 모든 변경은 이미 sticky 코멘트와 회귀 테스트 매트릭스에 의해 자동으로 검증되는 인프라 위에서 안전하게 진행될 수 있습니다.

---

## 부록 A — 주요 PR 인덱스

### 인프라 / Release 파이프라인 (윤민우)
- [#52](https://github.com/ymw0407/cargo-chronoscope/pull/52) docs(readme): refresh Status section
- [#53](https://github.com/ymw0407/cargo-chronoscope/pull/53) chore: fix .gitignore typo
- [#54](https://github.com/ymw0407/cargo-chronoscope/pull/54) chore(ci): bump action default version
- [#59](https://github.com/ymw0407/cargo-chronoscope/pull/59) fix(ci): forked-PR sticky comment via workflow_run
- [#62](https://github.com/ymw0407/cargo-chronoscope/pull/62) docs(readme): embed watch dashboard demo GIF
- [#64](https://github.com/ymw0407/cargo-chronoscope/pull/64) chore(release): add Windows target + macOS Intel cross-compile
- [#65](https://github.com/ymw0407/cargo-chronoscope/pull/65) chore: bump to v0.1.7
- [#80](https://github.com/ymw0407/cargo-chronoscope/pull/80) chore: bump to v0.1.8
- [#81](https://github.com/ymw0407/cargo-chronoscope/pull/81) chore: v0.1.9 — single-namespace tag consolidation

### Race condition fix
- [#34](https://github.com/ymw0407/cargo-chronoscope/pull/34) fix(tui): preserve build record after dashboard exit (R5 변종, 윤민우)
- [#55](https://github.com/ymw0407/cargo-chronoscope/pull/55) refactor(tui): exercise exit key wait (#37 회귀 테스트, txhno)
- [#77](https://github.com/ymw0407/cargo-chronoscope/pull/77) fix(main): wire SupervisorHandle cancel (R7, 유범익)
- [#79](https://github.com/ymw0407/cargo-chronoscope/pull/79) fix(persist): exclude failed builds from fetch_baseline (R5 보완, 유범익)

### 정책 / 거버넌스 변경
- 9d4a91e docs: rework role-ownership for OSS contributions (윤민우, 직접 main 푸시)
- [#61](https://github.com/ymw0407/cargo-chronoscope/pull/61) refactor: drop skeleton-phase #![allow(dead_code)] (윤민우)

---

## 부록 B — 참고 문서

- [`README.md`](../../README.md) — 사용자용 영문 문서
- [`CHANGELOG.md`](../../CHANGELOG.md) — Keep-a-Changelog 형식 release note
- [`CONTRIBUTING.md`](../../CONTRIBUTING.md) — 외부 컨트리뷰션 가이드
- [`docs/internal/DESIGN.md`](DESIGN.md) — 초기 설계 문서 (한국어)
- [`docs/internal/CONCURRENCY.md`](CONCURRENCY.md) — 동시성 race condition 원본 분석 (한국어)
- [`docs/internal/ROLE_OWNERSHIP.md`](ROLE_OWNERSHIP.md) — 3인 팀 역할 분담 (현재는 historical)
- [`docs/internal/PROJECT_HISTORY.md`](PROJECT_HISTORY.md) — 프로젝트 진행 기록 (영문)

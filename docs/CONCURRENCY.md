# 동시성 & Race Condition 분석

cargo-chrono는 5개의 async task가 동시에 동작하는 파이프라인이다.
이 문서는 **예상되는 race condition과 대응 전략**을 모듈/구현자별로 정리한다.
구현 전에 이 문서를 읽고, 구현 후에는 체크리스트의 각 항목이 처리됐는지 확인한다.

## 0. 아키텍처 복습

```
Record:  Supervisor ──mpsc<String>──▶ Parser ──mpsc<BuildEvent>──▶ Persister ──▶ SQLite
Watch:   Supervisor ──▶ Parser ──▶ Broker ─┬──▶ Persister ──▶ SQLite
                                            └──▶ TUI      ──▶ Terminal
         + CancellationToken은 main.rs에서 모든 task로 전파
```

공유 자원:

| 자원 | 접근자 | 동기화 |
|------|--------|--------|
| `rusqlite::Connection` | Persister(write), TUI(read via trait) | `tokio::sync::Mutex` |
| cargo stdout pipe | Supervisor | OS 커널 버퍼 |
| Terminal stdout/stderr | TUI, main.rs, supervisor(cargo inherit) | 명시적 순서 제어 필요 |
| BuildEvent 스트림 | Broker → 다수 subscriber | mpsc bounded, 각 subscriber별 독립 채널 |

## 1. 심각도 분류

- **[HIGH]** — 빌드 데이터 정합성을 깨거나, 프로세스를 hang/crash시킴
- **[MID]** — UX/성능이 악화되지만 데이터는 정상
- **[LOW]** — 엣지 케이스에서만 발생, 관찰이 어려움

---

## 2. 프로세스 레벨 Race Conditions

### R1. [HIGH] Supervisor → Parser 백프레셔 데드락

**시나리오**:
- Parser의 mpsc 버퍼(1024)가 가득 참
- Supervisor가 `tx.send(line).await`에서 블록
- cargo stdout pipe의 OS 버퍼(보통 64KB)가 차면 cargo의 write syscall도 블록
- **cargo 빌드가 실제로 멈춘다**

**증상**: "빌드가 진행 중인데 cargo-chrono가 멈춘 것처럼 보임"

**대응**:
- Parser는 절대 blocking I/O를 하지 않는다. JSON 파싱만 담당(CPU-only).
- DB 쓰기, 파일 I/O 등은 Parser 다음 단계(Persister)에서 한다.
- Persister가 느려지는 건 어쩔 수 없지만, 그 지연이 cargo 프로세스까지 전파되지 않도록 Parser가 방어벽 역할을 한다.

**책임**: Integrator (parser 구현 시 주의), Data (persister가 과도하게 느려지지 않게)

**테스트**: Parser 단위 테스트에서 임의로 큰 JSON 배치를 한 번에 밀어넣었을 때 deadlock 없이 처리하는지 확인.

---

### R2. [MID] Ctrl-C vs cargo 정상 종료 경합

**시나리오**:
- 사용자가 빌드 끝 무렵 Ctrl-C를 누름
- `CancellationToken::cancel()` → `SupervisorHandle::cancel()` → `child.kill()`
- 동시에 cargo는 정상적으로 exit
- `child.kill()`이 이미 종료된 프로세스에 호출되면 OS 에러 반환

**대응**:
```rust
// Supervisor::cancel()
let _ = self.child.kill(); // 이미 종료된 경우 에러 무시
```

**책임**: Integrator (supervisor)

---

### R3. [MID] Ctrl-C의 프로세스 그룹 전파

**시나리오**:
- 터미널 포그라운드의 프로세스 그룹 전체에 SIGINT 전달
- cargo-chrono와 자식 cargo 모두 SIGINT 수신
- cargo가 먼저 죽으면서 stdout 버퍼의 마지막 JSON 라인이 잘릴 수 있음
- Parser가 불완전한 JSON에 대해 에러 뱉고 종료

**대응** (MVP):
- Parser는 malformed JSON을 조용히 무시 (이미 테스트에 반영: `malformed_json_does_not_crash_parser`)
- 마지막 BuildFinished가 없어도 Persister는 `finalize_build(success=false)`로 마무리

**대응** (향후):
- `Command::process_group(0)`으로 cargo를 별도 프로세스 그룹에 spawn → SIGINT를 우리가 제어

**책임**: Integrator (supervisor, parser), Data (persister)

---

## 3. 채널 & 이벤트 순서 Race Conditions

### R4. [HIGH] Watch mode: 슬로우 서브스크라이버가 전체 파이프라인 지연

**시나리오**:
- Watch 모드에서 Broker가 Persister와 TUI 두 subscriber에 이벤트 fan-out
- TUI가 무거운 렌더링으로 느려지면 TUI 채널이 가득 참
- Broker가 `tx.send().await`로 TUI에 블록됨
- **그 사이 Persister에도 이벤트가 전달되지 않아 DB 기록도 지연**

**증상**: TUI 프레임 드랍이 DB 기록 지연으로 이어짐. 최악의 경우 빌드 완료 후에도 몇 초간 "기록 중".

**대응**:
```rust
// EventBroker::publish_loop 내부
for tx in &self.subscribers {
    match tx.try_send(event.clone()) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            // TUI 같은 "drop-on-full" 구독자: 이번 이벤트 스킵
            // 대신 로그로 기록 (관측용)
        }
        Err(TrySendError::Closed(_)) => {
            // 이 구독자는 죽음 → 제거 대상으로 마킹
        }
    }
}
```

→ `send().await`가 아니라 `try_send`를 써서 블록 제거. 가득 찬 채널은 프레임 드롭으로 처리.

**주의**: Persister의 이벤트는 **절대 드롭하면 안 된다**. Persister 채널만 `send().await`를 쓰고, TUI 채널만 `try_send`를 쓰는 식으로 구독자별 정책을 구분하는 것도 방법.

**책임**: Realtime (broker)

**테스트**: `broker::tests::dead_subscriber_does_not_block_others`가 부분적으로 커버. 슬로우(가득 찬) 구독자 케이스는 추가 테스트 필요.

---

### R5. [MID] Parser 내부 상태: 컴파일 시작 시점 모호성

**시나리오**:
- Cargo의 JSON 출력에는 "compilation-started"에 해당하는 이벤트가 없다.
  `compiler-artifact`는 **완료 시점**에만 emit된다.
- Parser가 `CompilationFinished`의 `duration`을 계산하려면 "언제 시작됐는지"를 추정해야 함.
- 추정 방법: 동일 crate의 `compiler-message`가 처음 나온 시점 또는 Parser 시작 시점
- 병렬 컴파일 중이면 여러 crate의 메시지가 섞여 나옴 → 추정 시점에 오차

**영향**: duration이 실제보다 과대평가될 수 있음 (최초 관측 시점 ~ 완료 시점).

**대응** (MVP): 근사치 허용. 팀 내부에서 같은 규칙을 쓰면 상대 비교(diff)는 의미 있음.

**대응** (향후): `cargo build --timings=json` 출력을 파싱하면 per-crate `start_time`/`end_time`을 cargo가 제공함. MVP 이후 고려.

**책임**: Integrator (parser)

---

### R6. [HIGH] BuildFinished가 미완료 compilation보다 먼저 오는 경우

**시나리오**:
- 정상 빌드: 모든 `compiler-artifact` → `build-finished`
- 비정상 종료 (Ctrl-C, 링크 에러 등): cargo가 중간에 `build-finished(success=false)`를 emit하고 미완료 crate는 completion 메시지가 안 나옴
- Parser 내부 HashMap에 "시작됐지만 끝나지 않은" compilation이 남음
- 그대로 두면 영원히 flush되지 않음

**대응**:
```rust
// Parser 내부
BuildEvent::BuildFinished { .. } => {
    // pending compilations는 모두 버림 (또는 success=false로 flush)
    pending_starts.clear();
    emit(event).await;
    break; // 이후 라인은 무시
}
```

**책임**: Integrator (parser)

---

### R7. [MID] mpsc 채널 종료 감지로 인한 BuildFinished 누락

**시나리오**:
- Supervisor가 cargo의 stdout EOF를 감지 → sender drop
- Parser의 `rx.recv()`가 `None` 반환
- 하지만 Parser가 아직 `build-finished` JSON 라인을 보지 못했을 수 있음 (버퍼에 남아 있거나, cargo가 마지막 라인을 출력하기 전에 exit)
- Persister는 `finalize_build`를 호출하지 않고 종료 → DB에 "unfinalized" 빌드 row 남음

**대응**:
- Parser: input close 시, `build-finished`를 안 받았으면 `BuildEvent::BuildFinished { success: false, .. }`를 합성해 emit.
- Persister: `rx.recv()` 종료 시점까지 `finalize_build`가 안 호출됐으면 `success=None`으로 마감.
- CLI `ls`: `success=None`은 `"???"`로 표시 (이미 `render_ls`에 반영됨).

**책임**: Integrator (parser), Data (persister)

---

## 4. DB 동시성 Race Conditions

### R8. [MID] `Mutex<Connection>` 경합 (read vs write)

**시나리오**:
- `tokio::sync::Mutex<Connection>`은 한 번에 한 접근만 허용
- Watch 모드에서 Persister가 `INSERT` 수행 중이면 TUI의 `fetch_baseline` 호출이 대기
- Persister가 compilation마다 INSERT, TUI가 compilation마다 baseline 조회 → 매번 경합

**영향**: TUI의 이상 감지가 지연. 최악의 경우 anomaly 표시가 빌드 종료 후에 나타남.

**대응**:
- SQLite는 WAL 모드에서 다수 reader + 단일 writer를 동시에 허용
- TUI 전용 **read-only 커넥션**을 별도로 열어 `Arc<RoConnection>`으로 주입
- Persister만 write 커넥션을 소유

**설계 스케치**:
```rust
pub struct SqliteRepository {
    write_conn: Mutex<Connection>,
    // 여러 개의 read 커넥션을 pool에 보관하거나, Clone 가능하게
    read_conn: Mutex<Connection>, // read-only open
}
```

**대응 (MVP)**: 단일 Mutex 유지, 성능이 문제되면 분리. WAL 모드는 이미 켜져 있음 (`SqliteRepository::open`).

**책임**: Data

---

### R9. [HIGH] 트랜잭션 경계 부재로 인한 orphan build row

**시나리오**:
- `begin_build` → `record_compilation`... → `finalize_build`는 **논리적으로 하나의 트랜잭션**
- 현재 구조에서는 각각 별도 commit
- 빌드 중 프로세스 크래시, 강제 kill, OOM 등으로 Persister가 중단되면:
  - `builds` row: 있음 (success=NULL)
  - `crate_compilations`: 일부만
  - `finalize_build`: 호출 안 됨 → `finished_at`, `total_duration_ms`도 NULL

**영향**: `ls`에 "???" 상태 빌드가 누적. `diff`로 비교하면 데이터 부정확.

**대응** (MVP):
- 이 상태를 정상 상태로 인정. `render_ls`는 이미 `Some(true)/Some(false)/None`을 `ok/FAIL/???`로 표시.
- 사용자가 명시적으로 `ls`에서 ???를 보고 이해한다.

**대응** (향후):
- 프로세스 시작 시, 마지막 세션에서 unfinalized 빌드를 자동 정리하는 recovery step 추가
- 또는 빌드 전체를 SQLite transaction으로 감싸기 (단, 트랜잭션 중에는 다른 read/write가 대기하므로 TUI 경험 악화)

**책임**: Data

---

### R10. [LOW] Baseline 계산에 현재 빌드가 포함되는 self-pollution

**시나리오**:
- Watch 모드에서 Persister가 `record_compilation("serde", 5.0s)` 기록
- 직후 TUI가 같은 `serde`에 대해 `fetch_baseline` 호출
- Baseline 통계에 "방금 기록한 값"이 포함됨 → 자기 자신과 비교
- Anomaly 분류가 정확하지 않음 (특히 과거 샘플이 적을 때 심함)

**대응**:
- `fetch_baseline` 시그니처에 `exclude_build_id: Option<BuildId>` 추가
- TUI는 현재 진행 중인 빌드의 ID를 전달해 제외

**대응 (MVP)**: 데이터가 10건 이상 쌓이면 1건 섞여도 큰 영향 없음. 문제 인지하되 구현은 보류.

**책임**: Data

---

## 5. 터미널 상태 Race Conditions

### R11. [HIGH] TUI raw mode 복원 실패

**시나리오**:
- TUI가 `crossterm::terminal::enable_raw_mode()` 호출 후 렌더링 시작
- 렌더링 코드 내부에서 panic 발생 (인덱스 범위 초과 등)
- panic unwinding 중에 Drop 가드가 호출되어야 raw mode가 복원됨
- 하지만 **프로세스가 abort되거나 stack overflow**면 Drop이 안 불림
- 터미널이 raw mode로 남아 셸 복귀 시 echo 없음, 입력 가시성 없음 → 사용자 터미널 망가짐

**대응**:
```rust
// run_tui 시작 부분
let original_hook = std::panic::take_hook();
std::panic::set_hook(Box::new(move |panic_info| {
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(std::io::stderr(), crossterm::terminal::LeaveAlternateScreen);
    original_hook(panic_info);
}));

// 또한 RAII guard로도 이중 안전망
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}
```

**책임**: Realtime (tui)

---

### R12. [MID] Shutdown 시 TUI 종료와 stdout 메시지의 인터리브

**시나리오**:
- Ctrl-C → cancel token → 모든 task 종료 신호
- TUI가 정리 중(alternate screen leave)에 main.rs의 `println!("Build #N recorded.")`가 먼저 stdout에 도달
- 사용자 화면에 이상한 문자열 섞임

**대응**:
- shutdown 순서를 명시적으로 제어:
  1. cancel token fire
  2. TUI가 먼저 raw mode disable + alternate screen leave (await)
  3. Persister drain 완료 대기
  4. 그제서야 main.rs가 final 메시지 println

**구현**:
```rust
let (_, persister_result, _) = tokio::try_join!(
    broker.publish_loop(...),
    persist::run_persister(...),
    tui::run_tui(...), // 내부에서 cleanup까지 await
)?;
// 여기에 도달하면 TUI는 이미 cleanup 완료
println!("Build {} recorded.", persister_result);
```

`main.rs`의 `cmd_watch`가 이미 이 순서를 따른다. 변경 시 주의.

**책임**: Integrator (main.rs), Realtime (tui가 Drop까지 await 보장)

---

## 6. 종료 & 취소 프로토콜

전체 shutdown 단계 (Watch 모드 기준):

```
[1] 트리거:
    - 사용자 Ctrl-C   ──▶ cancel_token.cancel()
    - cargo 정상 종료 ──▶ supervisor channel close
    - q 키 입력       ──▶ cancel_token.cancel()
    - 에러 발생       ──▶ try_join! 에서 다른 task 자동 cancel

[2] 전파:
    Supervisor  : child.kill() (이미 exit이면 무시)
    Parser      : input close 감지 → pending flush → output close
    Broker      : input close OR cancel → subscriber들에 drop
    Persister   : input close → finalize_build(success 확정)
    TUI         : cancel 수신 → raw mode disable → alternate screen leave

[3] 조인:
    try_join!이 모든 task 완료 대기
    TUI cleanup 완료 확인

[4] 최종 출력:
    println!("Build #{} recorded.", build_id);
```

**불변식**:
- `run_tui`는 반환 전에 반드시 raw mode를 복원해야 한다.
- `run_persister`는 반환 시 항상 마지막 `finalize_build`를 호출한 상태여야 한다 (성공/실패 무관).
- `cargo` 프로세스는 cargo-chrono보다 먼저 종료돼야 한다 (좀비 방지).

---

## 7. 테스트 전략

### 단위 테스트로 커버 가능한 것
- [R6] Parser가 BuildFinished 받은 뒤 pending compilation 처리: `parser::tests`에 추가 가능
- [R7] Persister가 BuildFinished 없이 input close 시 success=None 마감: `persist::tests`에 추가 가능
- [R4] Broker 가득 찬 구독자 처리: `broker::tests`에 추가 가능 (작은 버퍼로 재현)
- [R2] Supervisor의 kill idempotency: `supervisor::tests`에 추가 가능

### 통합 테스트가 필요한 것
- [R1] 백프레셔 데드락: 실제 큰 프로젝트 빌드로만 재현 가능
- [R8] DB 경합: 동시 read/write 부하로 재현
- [R11] 터미널 복원: 수동 테스트 (panic 주입 → 셸에서 echo 동작 확인)

### `loom`은 필요한가?
필요 없다. 공유 상태가 대부분 mpsc 채널과 Mutex이고, 로직이 순차적이다.
loom은 lock-free 자료구조 검증용이다. tokio의 Mutex/mpsc는 이미 프로덕션 검증된 것을 쓴다.

---

## 8. 구현 체크리스트

PR 머지 전 자기 역할의 항목이 처리됐는지 확인.

### Integrator
- [ ] Parser가 blocking I/O 없음 (R1)
- [ ] Supervisor의 `cancel()`이 이미 종료된 child에 idempotent (R2)
- [ ] Parser가 malformed JSON을 무시 (R3, 테스트 있음)
- [ ] Parser가 BuildFinished 뒤 pending compilation 처리 (R6)
- [ ] Parser가 input close 시 synthetic BuildFinished emit (R7)
- [ ] `cmd_watch`의 shutdown 순서 (R12)

### Data
- [ ] `SqliteRepository::open`이 WAL 모드 활성화 (R8, 이미 구현됨)
- [ ] `run_persister`가 항상 finalize_build 호출 (R7, R9)
- [ ] `ls`가 success=None을 `???`로 표시 (R9, 이미 구현됨)
- [ ] (선택) `fetch_baseline`에 exclude_build_id 추가 (R10)

### Realtime
- [ ] `EventBroker::publish_loop`가 `try_send`로 구독자별 드롭 처리 (R4)
- [ ] TUI가 panic hook에서 raw mode 복원 (R11)
- [ ] TUI가 Drop guard로 이중 안전망 (R11)
- [ ] `run_tui`가 반환 전 cleanup 완료 (R12)
- [ ] TUI 렌더링 루프가 cancel token 60fps 이내 주기로 체크 (R12)

---

## 9. 참고

- Tokio Async Book — Cancellation & Shutdown 패턴
- SQLite WAL 모드 공식 문서 (https://www.sqlite.org/wal.html)
- `tokio::sync::mpsc` — `send().await` vs `try_send` 차이
- crossterm README — raw mode 복원 모범 사례

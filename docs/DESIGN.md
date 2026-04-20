# cargo-chrono 설계 문서

## 1. 프로젝트 개요

`cargo-chrono`는 Rust의 빌드 도구 Cargo가 생성하는 빌드 이벤트 스트림을 수집·저장·분석하여
빌드 성능을 관측하는 CLI 도구이다.

Cargo는 `--message-format=json` 플래그를 통해 빌드 과정의 모든 이벤트를 JSON 스트림으로
내보낸다. cargo-chrono는 이 스트림을 파싱하여 로컬 SQLite 데이터베이스에 기록하고,
과거 빌드와의 비교(diff) 및 실시간 모니터링(watch) 기능을 제공한다.

## 2. 문제 정의

### 현재 상황
- Rust 프로젝트의 빌드 시간은 프로젝트가 커질수록 증가하지만, 개발자는 "어떤 crate가
  얼마나 오래 걸리는지", "지난주보다 빌드가 느려졌는지" 등을 직관에 의존해 판단한다.
- `cargo build --timings`는 단일 빌드의 HTML 리포트만 생성하며, 과거 빌드와의 비교나
  실시간 모니터링은 지원하지 않는다.
- Rust 프로젝트 목표 2025 H2에서 "외부 도구가 역사적 추세를 분석하는 역할"을 명시하였으나,
  아직 이를 구현한 도구는 없다.

### 해결 목표
1. 빌드 이벤트를 자동으로 수집하여 과거 데이터를 축적한다.
2. 임의의 두 빌드를 비교해 성능 변화를 정량적으로 보여준다.
3. 빌드 중 실시간으로 진행 상황과 이상 징후를 시각화한다.

## 3. 핵심 컨셉

### 3.1 BuildEvent 스트림
Cargo의 JSON 출력을 파싱하여 내부 `BuildEvent` enum으로 변환한다.
이벤트 순서는 항상 `BuildStarted → (CompilationStarted/Finished)* → BuildFinished`이다.

### 3.2 기록(Record) vs 관측(Watch)
- **Record**: 빌드를 실행하고 DB에만 저장. CI 환경이나 headless 용도.
- **Watch**: 빌드를 실행하면서 동시에 TUI 대시보드를 띄움. 개발자 데스크톱 용도.

### 3.3 2σ 이상 감지
같은 crate의 과거 컴파일 시간 분포(평균, 표준편차)를 기준으로,
현재 빌드에서 mean ± 2σ를 벗어나는 crate를 Slower/Faster로 분류한다.

### 3.4 Critical Path
빌드의 의존성 그래프에서 가장 긴 경로(DAG longest path)를 계산하여
빌드 전체 시간을 결정하는 병목 체인을 식별한다.

## 4. 사용자 시나리오

### 시나리오 1: diff — "이번 PR이 빌드를 느리게 만들었나?"

```
# 1. main 브랜치에서 빌드 기록
$ git checkout main
$ cargo-chrono record -- --release
  ✓ Build #41 recorded (32.4s, 187 crates)

# 2. feature 브랜치에서 빌드 기록
$ git checkout feat/add-telemetry
$ cargo-chrono record -- --release
  ✓ Build #42 recorded (38.1s, 193 crates)

# 3. 비교
$ cargo-chrono diff 41 42
  Build #41 (main, 32.4s) → Build #42 (feat/add-telemetry, 38.1s)
  Total: +5.7s (+17.6%)

  Slower crates:
    opentelemetry-sdk    — (new)    4.2s
    opentelemetry-api    — (new)    1.8s
    my-app               — 2.1s → 3.4s  (+1.3s, +62%)

  Critical path changed:
    Before: serde → tokio → my-app (12.1s)
    After:  serde → tokio → opentelemetry-sdk → my-app (16.3s)
```

### 시나리오 2: watch — "빌드가 왜 이렇게 오래 걸리지?"

```
$ cargo-chrono watch -- --release

┌─ cargo-chrono watch ─────────────────────────────────────┐
│ Build #43  ▸ release  ▸ commit a1b2c3d                    │
│ Elapsed: 0:18 / ~0:33  ████████████░░░░░░░░ 55%          │
│                                                           │
│ Compiling now:                                            │
│  ▸ serde_derive  [8.2s]  ⚠ SLOW (mean 5.1s, +2.1σ)     │
│  ▸ tokio         [3.1s]  ✓ normal                         │
│  ▸ syn           [6.0s]  ✓ normal                         │
│                                                           │
│ CPU: 87%  │  Memory: 4.2 GB  │  Cores active: 8/10       │
│                                                           │
│ Recent:                                                   │
│  ✓ libc           0.4s   ✓ normal                         │
│  ✓ proc-macro2    1.2s   ✓ normal                         │
│  ✓ quote          0.8s   ● faster (mean 1.1s)            │
└───────────────────────────────── q: quit ─ ?: help ──────┘
```

## 5. 시스템 구조

```
                    ┌──────────────┐
  사용자 입력       │   main.rs    │  CLI 파싱, DI, task 조립
                    │  (Integrator)│
                    └──────┬───────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
     ┌──────────────┐ ┌────────┐ ┌──────────┐
     │  Supervisor   │ │  CLI   │ │   ...    │
     │ (cargo spawn) │ │(render)│ └──────────┘
     └──────┬───────┘ └────────┘
            │ mpsc<String>     (JSON 라인)
            ▼
     ┌──────────────┐
     │    Parser     │  JSON → BuildEvent 변환
     └──────┬───────┘
            │ mpsc<BuildEvent>
            ▼
     ┌──────────────┐
     │    Broker     │  1:N fan-out (watch 모드)
     └──┬────────┬──┘
        │        │
        ▼        ▼
  ┌──────────┐ ┌──────────┐
  │ Persister│ │   TUI    │  병렬 소비
  │  (Data)  │ │(Realtime)│
  └──────────┘ └──────────┘
        │              │
        ▼              ▼
  ┌──────────┐ ┌──────────┐
  │  SQLite  │ │ Terminal │
  └──────────┘ └──────────┘
```

## 6. 병렬성 아키텍처

Watch 모드에서는 최대 5개의 async task가 동시에 실행된다.

| # | Task | 역할 | 종료 조건 |
|---|------|------|----------|
| 1 | Supervisor | cargo 프로세스 관리, stdout → 채널 | cargo 프로세스 종료 |
| 2 | Parser | JSON 라인 → BuildEvent | 입력 채널 닫힘 |
| 3 | Broker | BuildEvent fan-out | 입력 채널 닫힘 |
| 4 | Persister | BuildEvent → SQLite | 입력 채널 닫힘 |
| 5 | TUI | 이벤트 수신 + 렌더링 루프 | 사용자 종료(q) 또는 CancellationToken |

**종료 흐름**:
1. Cargo 프로세스 종료 → Supervisor 채널 닫힘
2. Parser 입력 끝 → BuildFinished 발행 후 출력 채널 닫힘
3. Broker 입력 끝 → 모든 subscriber 채널 닫힘
4. Persister / TUI 입력 끝 → 정리 후 종료
5. 사용자가 q/Ctrl-C → CancellationToken으로 모든 task에 취소 전파

**Record 모드**는 Broker/TUI 없이 Supervisor → Parser → Persister 3단계만 실행.

## 7. 데이터 모델

### SQL 스키마

```sql
CREATE TABLE IF NOT EXISTS builds (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at      TEXT    NOT NULL,   -- ISO 8601
    finished_at     TEXT,
    commit_hash     TEXT,
    cargo_args      TEXT    NOT NULL,   -- JSON array
    profile         TEXT    NOT NULL,   -- "dev" | "release" | "custom"
    success         INTEGER,            -- 0 or 1
    total_duration_ms INTEGER
);

CREATE TABLE IF NOT EXISTS crate_compilations (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    build_id        INTEGER NOT NULL REFERENCES builds(id),
    crate_name      TEXT    NOT NULL,
    crate_version   TEXT,
    kind            TEXT    NOT NULL,   -- "lib" | "bin" | ...
    started_at      TEXT    NOT NULL,
    finished_at     TEXT    NOT NULL,
    duration_ms     INTEGER NOT NULL
);

CREATE INDEX idx_compilations_build ON crate_compilations(build_id);
CREATE INDEX idx_compilations_crate ON crate_compilations(crate_name);
```

### Baseline 계산 쿼리 (참고용)
```sql
SELECT
    crate_name,
    COUNT(*)        AS sample_count,
    AVG(duration_ms) AS mean,
    -- SQLite에는 STDDEV가 없으므로 수동 계산
    ...
FROM crate_compilations
WHERE crate_name = ?
GROUP BY crate_name;
```

## 8. MVP 범위

### 포함 (Week 1-2)
- [x] 프로젝트 스켈레톤
- [ ] `record` 명령: 빌드 실행 → DB 기록
- [ ] `ls` 명령: 빌드 목록 조회
- [ ] `diff` 명령: 두 빌드 비교 (crate별 시간 변화, 추가/제거)
- [ ] `watch` 명령: 실시간 TUI (컴파일 중 crate, 이상 감지, CPU/메모리)
- [ ] 2σ 기반 이상 감지
- [ ] Critical path 계산

### 제외 (향후)
- HTML/JSON 리포트 내보내기
- 원격 빌드 서버 지원
- 빌드 캐시 히트율 분석
- Cargo 공식 API 연동
- 증분 빌드 분석

## 9. 3인 팀 분업

### Integrator (공용 + 이벤트 생산자)
**소유**: `model/`, `cli/`, `supervisor/`, `parser/`, `main.rs`, `Cargo.toml`

핵심 책임:
- 모든 공용 타입 정의 및 유지
- Cargo 프로세스 관리 (spawn, stdout capture, 종료 처리)
- JSON 라인을 BuildEvent로 변환하는 파서
- CLI 파싱 및 출력 렌더링
- main.rs에서 전체 async task 조립

### Data 담당
**소유**: `persist/`, `diff/`

핵심 책임:
- SQLite 스키마 설계 및 마이그레이션
- BuildRepository trait 구현 (CRUD)
- run_persister: BuildEvent 스트림을 받아 순차적으로 DB 기록
- Baseline 통계 계산 (평균, 표준편차)
- compute_diff: 두 빌드 비교 알고리즘
- Critical path 계산 (DAG longest path)

### Realtime 담당
**소유**: `broker/`, `anomaly/`, `tui/`

핵심 책임:
- EventBroker: 1:N fan-out (watch 모드에서 persister와 TUI에 동시 전달)
- anomaly: 2σ 기반 이상 감지 (순수 함수)
- TUI 대시보드: 진행 중 crate, 이상 경고, CPU/메모리, ETA

## 10. 위험 요소

| 위험 | 영향 | 대응 |
|------|------|------|
| Cargo JSON 출력 형식 변경 | Parser 깨짐 | serde_json으로 유연하게 파싱, 알 수 없는 필드 무시 |
| SQLite 동시 쓰기 병목 | Persister 느려짐 | WAL 모드 사용, 배치 INSERT |
| TUI 렌더링이 이벤트 처리를 블로킹 | 이벤트 유실 | 별도 task로 분리, bounded channel |
| Critical path 계산 복잡도 | 대형 프로젝트에서 느림 | MVP는 단순 구현, 필요 시 최적화 |
| 팀원 간 인터페이스 불일치 | 통합 지연 | model/ 타입을 Day 1에 확정, PR 리뷰 필수 |

### 10.1 Race Condition 분석

5개 async task가 동시에 돌기 때문에 아래와 같은 race condition이 예상된다.
구현 전에 **[docs/CONCURRENCY.md](CONCURRENCY.md)** 를 읽고, PR 머지 전에 해당 문서의
"구현 체크리스트"로 검증한다.

주요 race condition 요약:

| ID | 심각도 | 내용 | 책임 |
|----|--------|------|------|
| R1 | HIGH | Supervisor → Parser 백프레셔 데드락 (cargo 프로세스가 stdout write에서 블록) | Integrator |
| R4 | HIGH | Watch 모드에서 슬로우 TUI 구독자가 Persister까지 지연시킴 | Realtime |
| R6 | HIGH | BuildFinished 이후 Parser 내부 pending compilation 처리 | Integrator |
| R7 | MID  | mpsc input close 감지 타이밍으로 인한 finalize_build 누락 | Integrator, Data |
| R9 | HIGH | 트랜잭션 경계 부재로 인한 orphan build row | Data |
| R11 | HIGH | TUI raw mode 복원 실패로 사용자 터미널 망가짐 | Realtime |
| R12 | MID  | Shutdown 시 TUI 종료와 stdout 메시지 인터리브 | Integrator, Realtime |

전체 12개 race condition과 대응 전략은 `docs/CONCURRENCY.md` 참조.

## 11. 발표 전략

### 데모 시나리오
1. 중간 크기 Rust 프로젝트(예: ripgrep)에서 `record`로 2회 빌드 기록
2. `diff`로 두 빌드 비교 결과 표시
3. `watch`로 실시간 TUI 대시보드 시연 — 느린 crate 경고 하이라이트

### 기술적 강조 포인트
- Rust의 async/await와 tokio를 활용한 5-task 병렬 파이프라인
- 2σ 통계 기반 이상 감지
- DAG longest path로 critical path 분석
- Cargo의 공식 프로젝트 목표에 부합하는 도구

## 12. 한계

- Cargo의 `--message-format=json` 출력에 의존하므로, 이 형식이 변경되면 파서 수정 필요
- 증분 빌드에서는 컴파일되지 않는 crate가 이벤트에 나타나지 않아 비교가 불완전할 수 있음
- Critical path 계산에 필요한 의존성 정보가 JSON 출력에 포함되지 않을 수 있음
  (이 경우 `cargo metadata`로 보완 필요)
- 단일 머신의 로컬 DB만 지원 — 팀 단위 추세 분석은 미지원

## 13. 기대 효과

1. **개발자 경험 향상**: "빌드가 느려진 것 같다"를 정량적으로 확인 가능
2. **PR 리뷰 품질 향상**: 새 의존성 추가가 빌드 시간에 미치는 영향을 수치로 평가
3. **CI 최적화 근거**: 어떤 crate가 빌드 시간의 병목인지 식별
4. **Rust 생태계 기여**: 공식 프로젝트 목표에 부합하는 오픈소스 도구

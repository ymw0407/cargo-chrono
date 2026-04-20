# cargo-chrono

> Cargo build performance observer — record, diff, and watch your Rust builds.

`cargo-chrono`는 Rust의 빌드 도구 Cargo가 내보내는 빌드 이벤트 스트림을 수집·저장·분석해
빌드 성능을 관측하는 CLI 도구입니다.

## 핵심 기능

| 명령어 | 설명 |
|--------|------|
| `cargo-chrono record [-- cargo args]` | 빌드를 실행하고 결과를 로컬 DB에 기록 |
| `cargo-chrono watch [-- cargo args]` | 빌드 + 실시간 TUI 대시보드 |
| `cargo-chrono ls [--last N]` | 기록된 빌드 목록 조회 |
| `cargo-chrono diff <before> <after>` | 두 빌드의 성능 비교 |

## 프로젝트 동기

Rust 공식 프로젝트 목표 **2025 H2 "Prototype Cargo build analysis"** (Help wanted)의
shiny future에 "외부 도구가 역사적 추세를 분석하는 역할"이 명시되어 있습니다.
`cargo-chrono`는 이 비전을 구체적인 CLI 도구로 구현합니다.

## 팀 구성 & 역할 분담

| 역할 | 소유 모듈 | 담당 |
|------|----------|------|
| **Integrator** | `model/`, `cli/`, `supervisor/`, `parser/`, `main.rs`, `Cargo.toml` | 공용 타입, 이벤트 생산자, 전체 조립 |
| **Data** | `persist/`, `diff/` | SQLite 저장소, 빌드 비교 분석 |
| **Realtime** | `broker/`, `anomaly/`, `tui/` | 이벤트 팬아웃, 이상 감지, TUI 대시보드 |

## 모듈 의존성 규칙

```
model/ ← 모든 모듈에서 import 가능 (역방향 금지)
Data 모듈 ↔ Realtime 모듈 : 서로 import 금지
Realtime → Data : BuildRepository trait만 사용 (구체 타입 아님)
main.rs : 전체 조립 (DI 컨테이너 역할)
```

## 역할별 계약

### Integrator가 제공하는 것
- `model::*` — 모든 공용 타입 (BuildEvent, BuildId, CrateId, Build, BuildDiff 등)
- `supervisor::spawn_build()` — Cargo 프로세스를 띄우고 stdout 라인을 채널로 전달
- `parser::run_parser()` — JSON 라인을 BuildEvent 스트림으로 변환
- `cli::Cli` — clap 기반 CLI 파싱
- `main.rs` — 모든 async task를 조립하고 Ctrl-C 핸들링

### Data가 제공하는 것
- `persist::BuildRepository` trait — 빌드 저장/조회 인터페이스
- `persist::SqliteRepository` — SQLite 구현체
- `persist::run_persister()` — BuildEvent 스트림을 받아 DB에 기록
- `diff::compute_diff()` — 두 빌드 비교 결과 생성

### Realtime이 제공하는 것
- `broker::EventBroker` — BuildEvent를 여러 subscriber에 fan-out
- `anomaly::classify()` — 2σ 기반 이상 감지 (순수 함수)
- `tui::run_tui()` — 실시간 빌드 모니터링 TUI

## 기술 스택

- **Runtime**: tokio (full) + tokio-util (CancellationToken)
- **CLI**: clap 4 (derive)
- **DB**: rusqlite (bundled, WAL mode)
- **TUI**: ratatui + crossterm
- **Serialization**: serde + serde_json
- **Error handling**: anyhow (공개 API) + thiserror (모듈별 에러 타입)
- **System info**: sysinfo

## 확정된 설계 결정

1. **DB 위치**: `<project_root>/.cargo-chrono/history.db`
2. **BuildId 발급**: Persister가 `BuildStarted` 이벤트를 받을 때 DB INSERT로 발급
3. **Compilation 매칭**: Parser가 start/finish를 내부적으로 매칭, `CompilationFinished`에 duration 포함
4. **채널**: bounded, 용량 1024
5. **에러**: `anyhow::Result` (공개 API), `thiserror` (모듈별 에러)
6. **비동기**: 모든 비동기 API는 tokio 런타임 기반

## 개발 일정 (2주)

### Week 1: 기반 구축
| Day | Integrator | Data | Realtime |
|-----|-----------|------|----------|
| 1 | model 타입 확정, supervisor 구현 | DB 스키마, SqliteRepository::open | ratatui hello world, broker 구현 |
| 2 | parser 구현 | run_persister 구현 | anomaly 구현 + 테스트 |
| 3 | main.rs record 명령 조립 | list_builds, fetch_build | TUI state 모델, 기본 렌더링 |
| 4 | 통합 테스트 (record → ls) | fetch_baseline | TUI에 anomaly 연동 |
| 5 | 버그 수정, 코드 리뷰 | 버그 수정, 코드 리뷰 | 버그 수정, 코드 리뷰 |

### Week 2: 고급 기능 + 마무리
| Day | Integrator | Data | Realtime |
|-----|-----------|------|----------|
| 6 | watch 명령 조립 | compute_diff 구현 | TUI 진행률 바, ETA 표시 |
| 7 | cli::render_diff 개선 | critical path 계산 | CPU/메모리 모니터 |
| 8 | 전체 통합 테스트 | diff 단위 테스트 | TUI 폴리싱 |
| 9 | README 업데이트, 문서화 | edge case 처리 | 종료 처리 안정화 |
| 10 | 발표 준비, 최종 점검 | 발표 준비, 최종 점검 | 발표 준비, 최종 점검 |

## Git 협업 규칙

1. **브랜치 전략**: `main` + feature 브랜치 (`feat/<role>/<topic>`)
2. **PR 규칙**: 최소 1명 리뷰 후 머지
3. **커밋 메시지**: Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`)
4. **충돌 방지**: 각 역할의 소유 모듈만 수정. `model/`은 Integrator가 소유하되 변경 시 팀 합의.
5. **CI**: 모든 PR은 `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` 통과 필수

## 데모 시뮬레이션 (예상 동작)

> 아래는 구현이 완료된 뒤 기대되는 사용자 경험입니다. 실제 출력 포맷은
> `src/cli/mod.rs`의 `render_ls` / `render_diff` 구현에서 확정되어 있으며,
> TUI 레이아웃은 `AGENTS.md`의 Realtime 섹션에 명시되어 있습니다.

### 1) `cargo-chrono record` — 빌드 기록

```console
$ cargo-chrono record -- --release
   Compiling proc-macro2 v1.0.86
   Compiling unicode-ident v1.0.13
   Compiling serde v1.0.210
   Compiling syn v2.0.77
   Compiling serde_json v1.0.132
   Compiling tokio v1.43.0
   Compiling my-app v0.1.0 (/Users/alice/projects/my-app)
    Finished `release` profile [optimized] target(s) in 42.7s
Build #12 recorded.
```

DB 파일 (`.cargo-chrono/history.db`)에 빌드 메타데이터와 crate별 컴파일 시간이 저장됩니다.

### 2) `cargo-chrono ls` — 최근 빌드 목록

```console
$ cargo-chrono ls --last 5
ID     Started              Profile  Duration   Status
------------------------------------------------------------
#12    2025-04-18T15:32:11  release  42.7s      ok
#11    2025-04-18T14:05:48  release  41.9s      ok
#10    2025-04-18T11:20:03  dev      8.2s       ok
#9     2025-04-17T18:44:29  release  35.6s      FAIL
#8     2025-04-17T17:12:15  release  35.1s      ok
```

빌드 기록이 없으면 `No builds recorded yet.`이 출력됩니다.

### 3) `cargo-chrono diff` — 두 빌드 비교

```console
$ cargo-chrono diff 11 12
Build #11 → Build #12
  Total: 41.9s → 42.7s (+0.8s, +1.9%)

  + serde_json v1.0.132 (new) 1.3s
  ~ my-app v0.1.0 12.4s → 14.0s (+1.6s, +12.9%)
  ~ tokio v1.43.0 3.8s → 4.1s (+0.3s, +7.9%)
  - old-dependency v0.3.2 (removed) 1.2s
  = serde v1.0.210 2.1s
  = syn v2.0.77 5.8s

Critical path (before): proc-macro2 → syn → serde → my-app
Critical path (after):  proc-macro2 → syn → serde → serde_json → my-app
```

기호 의미:
- `+` 새로 추가된 crate (after에만 존재)
- `-` 제거된 crate (before에만 존재)
- `~` 유의미하게 느려지거나 빨라진 crate (± 1% 초과)
- `=` 사실상 동일한 crate

crate 변경 목록은 `abs_delta_ms` 내림차순으로 정렬돼, 가장 큰 영향이 위에 옵니다.

### 4) `cargo-chrono watch` — 실시간 TUI 대시보드

`watch`는 빌드가 진행되는 동안 터미널을 장악하고, Record처럼 DB에도 동시 저장합니다.
`q` 또는 `Ctrl-C`로 종료합니다.

```
┌─ cargo-chrono ─────────────────────────────────────────────────────────┐
│ Build #13 (release)  •  commit 4899d16  •  elapsed 00:28               │
│ [████████████████████████░░░░░░░░░░░░░░░░░░░] 142/237 crates  ETA 19s │
├─ Active compilations ──────────────────────────────────────────────────┤
│  ▶ serde_derive v1.0.210       12.4s   ⚠ slower (baseline 7.1s ±0.8)   │
│  ▶ tokio v1.43.0                3.2s   · normal (baseline 3.8s ±0.4)   │
│  ▶ clap_derive v4.5.20          0.9s   · normal                        │
│  ▶ my-app (build-script)        0.3s   ? unknown  (no baseline)        │
├─ Recently finished (last 5) ───────────────────────────────────────────┤
│  ✓ syn v2.0.77                  5.82s  normal                          │
│  ✓ proc-macro2 v1.0.86          2.14s  normal                          │
│  ✓ unicode-ident v1.0.13        0.41s  ↓ faster (baseline 0.6s)        │
│  ✓ quote v1.0.37                0.98s  normal                          │
│  ✓ serde v1.0.210               2.09s  normal                          │
├─ System ───────────────────────────────────────────────────────────────┤
│  CPU:  ██████████████████░░  87%    Memory:  4.8 GiB / 16 GiB          │
└─────────────────────────────────────────[q] quit  [Ctrl-C] interrupt ──┘
```

이상 감지 아이콘:
- `⚠ slower` — 평균 + 2σ 초과 (`anomaly::classify`)
- `↓ faster` — 평균 − 2σ 미만
- `· normal` — 정상 범위
- `? unknown` — baseline이 없는 신규 crate

빌드가 끝나면 TUI가 닫히고 (터미널 raw mode 복원), 콘솔에 `Build #13 recorded.`가 출력됩니다.

### 데모 플로우 (발표용 시나리오)

```bash
# 1. 한 번 빌드해서 baseline을 만든다
cargo-chrono record -- --release

# 2. 의존성을 일부러 추가/제거한 뒤 다시 빌드
cargo add serde_json
cargo-chrono record -- --release

# 3. 두 빌드를 비교
cargo-chrono ls --last 2
cargo-chrono diff 11 12
```

TUI 스트레스 테스트: 느린 의존성(`syn`, `serde_derive`)이 들어간 프로젝트에서
`cargo clean && cargo-chrono watch -- --release`로 스타트업 빌드의 병목 구간을 실시간 관찰합니다.

## 빌드 & 실행

```bash
# 빌드
cargo build

# 빌드 기록
cargo run -- record -- --release

# 빌드 목록
cargo run -- ls --last 5

# 빌드 비교
cargo run -- diff 1 2

# 실시간 모니터링
cargo run -- watch -- --release
```

## 향후 과제

- [ ] Cargo의 공식 build analysis API가 안정화되면 연동
- [ ] HTML/JSON 리포트 내보내기
- [ ] 원격 빌드 서버 지원
- [ ] 빌드 캐시 히트율 분석
- [ ] 증분 빌드 vs 클린 빌드 비교
- [ ] GitHub Actions 연동 (CI 빌드 시간 추적)

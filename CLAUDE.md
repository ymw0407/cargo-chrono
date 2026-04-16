# CLAUDE.md — cargo-chrono

이 파일은 AI 코딩 어시스턴트(Claude Code 등)가 이 프로젝트에서 작업할 때 따라야 할 규칙과 컨텍스트입니다.

## 프로젝트 개요

cargo-chrono는 Rust의 Cargo 빌드 이벤트 스트림을 수집·저장·분석하는 CLI 도구입니다.
4개 명령(record, watch, ls, diff)을 제공하며, 3인 팀이 모듈별로 분리 개발합니다.

## 빌드 & 검증 명령

```bash
cargo check                          # 컴파일 확인
cargo test                           # 테스트 실행
cargo clippy -- -D warnings          # lint (경고를 에러로)
cargo fmt --check                    # 포맷 확인
cargo run --example ratatui_hello    # TUI 예제 실행
```

PR 제출 전 반드시 `cargo fmt --check && cargo clippy -- -D warnings && cargo test`를 모두 통과해야 합니다.

## 모듈 소유권 (절대 규칙)

| 역할 | 소유 모듈 |
|------|----------|
| **Integrator** | `src/model/`, `src/cli/`, `src/supervisor/`, `src/parser/`, `src/main.rs`, `Cargo.toml` |
| **Data** | `src/persist/`, `src/diff/` |
| **Realtime** | `src/broker/`, `src/anomaly/`, `src/tui/` |

**자기 소유가 아닌 모듈의 코드를 수정하지 마세요.** 다른 역할의 모듈 변경이 필요하면 해당 담당자에게 요청하거나 이슈를 생성하세요.

## 의존성 방향 (절대 규칙)

```
model/ ← 모든 모듈에서 import 가능 (model/은 다른 모듈을 import 금지)
Data ↔ Realtime : 서로 직접 import 금지
Realtime → Data : persist::BuildRepository trait만 import 가능 (SqliteRepository 직접 import 금지)
main.rs : 전체 모듈을 조립하는 유일한 장소
```

이 의존성 방향을 위반하는 코드를 작성하지 마세요. `use crate::tui` 같은 import가 `persist/` 안에 있으면 안 됩니다.

## 코드 컨벤션

### 에러 처리
- 공개 API 반환: `anyhow::Result<T>`
- 모듈 내부 에러 타입: `thiserror`로 정의
- `unwrap()`/`expect()`는 테스트 코드에서만 허용. 프로덕션 코드에서는 `?` 연산자 사용.

### 비동기
- 모든 비동기 코드는 tokio 런타임 기반
- trait에 async fn이 필요하면 `#[async_trait]` 사용
- 채널은 `tokio::sync::mpsc` bounded, 기본 용량 1024
- 취소는 `tokio_util::sync::CancellationToken`으로 전파

### 스타일
- `cargo fmt`을 따름 (기본 rustfmt 설정)
- 모든 public 함수, struct, enum, trait에 `///` doc comment 필수
- doc comment에 `# Arguments`, `# Returns`, `# Errors` 섹션으로 계약 명시
- 모듈 파일 최상단에 `//!` 모듈 doc comment 필수

### 커밋
- Conventional Commits 형식: `<type>(<scope>): <description>`
- type: feat, fix, refactor, test, docs, chore, perf, style
- scope: 모듈 이름 (model, cli, supervisor, parser, persist, diff, broker, anomaly, tui, main)
- 예: `feat(persist): implement begin_build with SQLite INSERT`
- 상세 규칙: `.github/COMMIT_CONVENTION.md` 참고

### 브랜치
- `feat/<role>/<topic>`, `fix/<role>/<topic>`, `test/<role>/<topic>`
- 예: `feat/data/sqlite-crud`, `fix/realtime/tui-crash-on-exit`

## 핵심 아키텍처 패턴

### 이벤트 파이프라인 (Record 모드)
```
Supervisor → mpsc<String> → Parser → mpsc<BuildEvent> → Persister → DB
```

### 이벤트 파이프라인 (Watch 모드)
```
Supervisor → Parser → Broker ─┬→ Persister → DB
                               └→ TUI → Terminal
```

### BuildEvent 스트림 계약
- 첫 이벤트: 반드시 `BuildStarted`
- 마지막 이벤트: 반드시 `BuildFinished`
- `CompilationFinished`에는 `duration`, `started_at`, `finished_at`가 항상 포함

### DB 위치
`<project_root>/.cargo-chrono/history.db` (SQLite, WAL mode)

### BuildId 발급
Persister가 `BuildStarted` 이벤트를 받을 때 DB INSERT → AUTOINCREMENT로 발급

## todo!() 스텁 상태

현재 스켈레톤 단계입니다. 다음 함수들이 실제로 구현되어 있습니다:
- `anomaly::classify()`, `anomaly::classify_in_progress()` + 테스트 8개
- `persist::SqliteRepository::open()` (DB 열기, WAL, 마이그레이션)
- `persist::migrations::run_migrations()` (DDL)
- `cli::render_ls()`, `cli::render_diff()` (텍스트 출력)
- `src/main.rs` 전체 흐름 (파싱 → 분기 → task 조립)

나머지는 `todo!()` 스텁입니다. `#![allow(dead_code)]`가 `main.rs`에 설정되어 있으며, 모듈 구현이 완료되면 제거해야 합니다.

## 테스트 작성 규칙

- 구현한 모든 public 함수에 단위 테스트 작성
- 테스트는 같은 파일의 `#[cfg(test)] mod tests {}` 블록에 위치
- 통합 테스트는 `tests/` 디렉터리
- 테스트 픽스처는 `tests/fixtures/` — `sample_output.jsonl`이 여기 들어갈 예정
- DB 테스트는 `tempfile::TempDir`로 임시 DB 생성
- `tokio::test`로 비동기 테스트

## 주의사항

- `cargo_metadata` 크레이트를 사용하지 마세요. `serde_json`으로 직접 파싱합니다.
- `rusqlite::Connection`은 `Sync`이 아닙니다. `tokio::sync::Mutex`로 감싸서 사용합니다.
- TUI는 raw mode를 사용합니다. panic 시에도 터미널 복원이 보장되어야 합니다.
- `Cargo.toml` 수정은 Integrator 역할만 합니다. 의존성 추가가 필요하면 Integrator에게 요청하세요.

## 참고 문서

- `docs/DESIGN.md` — 전체 설계 (시나리오, 아키텍처, 스키마, 분업)
- `docs/ONBOARDING.md` — 역할별 Day 1 체크리스트
- `.github/COMMIT_CONVENTION.md` — 커밋 컨벤션 상세
- `.github/CONTRIBUTING.md` — 기여 가이드

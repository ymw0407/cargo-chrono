# Contributing Guide

cargo-chrono에 기여해주셔서 감사합니다.

## 개발 환경 설정

```bash
# 저장소 클론
git clone <repo-url>
cd cargo-chrono

# 빌드 확인
cargo check

# 테스트 실행
cargo test

# Lint 확인
cargo clippy -- -D warnings

# 포맷 확인
cargo fmt --check
```

## 워크플로우

### 1. 이슈 확인

작업 전 관련 이슈가 있는지 확인합니다. 없으면 이슈를 먼저 생성하세요.

### 2. 브랜치 생성

```bash
git checkout -b feat/<role>/<topic>
# 예: git checkout -b feat/data/sqlite-crud
```

브랜치 네이밍은 [COMMIT_CONVENTION.md](COMMIT_CONVENTION.md)을 참고하세요.

### 3. 개발

- **자기 소유 모듈만 수정합니다.** 다른 역할의 모듈 수정이 필요하면 해당 담당자와 협의하세요.
- `model/` 타입 변경은 팀 전체 합의가 필요합니다.
- `Cargo.toml` 수정은 Integrator를 통해서만 합니다.

### 4. 커밋

[Conventional Commits](COMMIT_CONVENTION.md) 규칙을 따릅니다.

```bash
git add <files>
git commit -m "feat(persist): implement begin_build with SQLite INSERT"
```

### 5. PR 전 체크리스트

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

세 명령이 모두 통과해야 CI도 통과합니다.

### 6. PR 생성

- PR 템플릿을 채워주세요.
- 관련 이슈를 연결하세요 (`Closes #N`).
- 최소 1명의 리뷰를 받아야 머지할 수 있습니다.

## 모듈 소유권

| 역할 | 소유 모듈 |
|------|----------|
| Integrator | `model/`, `cli/`, `supervisor/`, `parser/`, `main.rs`, `Cargo.toml` |
| Data | `persist/`, `diff/` |
| Realtime | `broker/`, `anomaly/`, `tui/` |

### 의존성 규칙

```
model/ ← 모든 모듈에서 import 가능 (역방향 금지)
Data ↔ Realtime : 서로 import 금지
Realtime → Data : BuildRepository trait만 사용
```

## 코드 스타일

- `cargo fmt`으로 자동 포맷팅
- `cargo clippy -- -D warnings`으로 lint 통과
- 모든 public API에 doc comment 필수
- 에러 처리: 공개 API는 `anyhow::Result`, 모듈 내부는 `thiserror`

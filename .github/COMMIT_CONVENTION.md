# Commit Convention

이 프로젝트는 [Conventional Commits](https://www.conventionalcommits.org/ko/) 규칙을 따릅니다.

## 형식

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

## Type

| Type | 설명 |
|------|------|
| `feat` | 새로운 기능 추가 |
| `fix` | 버그 수정 |
| `refactor` | 기능 변경 없는 코드 개선 |
| `test` | 테스트 추가 또는 수정 |
| `docs` | 문서 변경 |
| `chore` | 빌드, CI, 의존성 등 기타 변경 |
| `perf` | 성능 개선 |
| `style` | 코드 포맷팅 (기능 변경 없음) |

## Scope

모듈 이름을 scope로 사용합니다.

| Scope | 소유 역할 |
|-------|----------|
| `model` | Integrator |
| `cli` | Integrator |
| `supervisor` | Integrator |
| `parser` | Integrator |
| `persist` | Data |
| `diff` | Data |
| `broker` | Realtime |
| `anomaly` | Realtime |
| `tui` | Realtime |
| `main` | Integrator |
| `ci` | 공용 |
| `docs` | 공용 |

## 예시

```
feat(supervisor): implement cargo process spawn and stdout streaming

Spawns `cargo build --message-format=json-render-diagnostics` as a child
process and streams stdout line-by-line through a bounded mpsc channel.

Closes #12
```

```
fix(persist): handle empty builds table on first run

The list_builds query was failing when the builds table had no rows.
Added a check for empty results before attempting to map rows.
```

```
test(anomaly): add edge case tests for zero std_dev

Covers the case where all historical compilation times are identical,
resulting in std_dev = 0.
```

```
refactor(model): rename CompilationRecord to CrateCompilation

Aligns naming with the design document terminology.

BREAKING CHANGE: CompilationRecord is now CrateCompilation
```

## 규칙

1. **제목은 50자 이내**, 영문 소문자로 시작, 마침표 없음
2. **본문은 72자에서 줄바꿈** (선택사항)
3. **Breaking change**가 있으면 footer에 `BREAKING CHANGE:` 명시
4. **이슈 연결**: `Closes #N`, `Fixes #N`, `Refs #N`
5. 하나의 커밋에 하나의 논리적 변경만 포함

## 브랜치 네이밍

```
feat/<role>/<topic>     # 새 기능: feat/data/sqlite-crud
fix/<role>/<topic>      # 버그 수정: fix/realtime/tui-crash-on-exit
refactor/<role>/<topic> # 리팩터링: refactor/integrator/parser-error-handling
docs/<topic>            # 문서: docs/update-design
test/<role>/<topic>     # 테스트: test/anomaly/edge-cases
```

예시:
- `feat/data/sqlite-crud`
- `feat/realtime/broker-publish-loop`
- `fix/integrator/supervisor-kill-signal`

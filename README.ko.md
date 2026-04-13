🇺🇸 [English](README.md)

<div align="center">

<h1 align="center">
  bw-engine
</h1>

<p align="center">
  <em>StarCraft: Brood War 리플레이 파서 및 웹용 게임 엔진.</em><br>
  <em>리플레이를 파싱하고. 게임을 시뮬레이션하고. 브라우저에서 렌더링합니다.</em>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/">
    <img alt="Rust" src="https://img.shields.io/badge/Rust-1.85+-000000?logo=rust&logoColor=white&style=for-the-badge">
  </a>
  <a href="https://webassembly.org/">
    <img alt="WASM" src="https://img.shields.io/badge/WebAssembly-322KB-654ff0?logo=webassembly&logoColor=white&style=for-the-badge">
  </a>
  <a href="LICENSE">
    <img alt="License" src="https://img.shields.io/badge/License-MIT-c6a0f6?style=for-the-badge">
  </a>
</p>

<p align="center">
  <a href="docs/getting-started.md">시작하기</a> &bull;
  <a href="docs/api-reference.md">API 레퍼런스</a> &bull;
  <a href="docs/game-data-files.md">게임 데이터 파일</a>
</p>

</div>

---

StarCraft: Brood War `.rep` 파일을 파싱하고 WebAssembly용으로 BW 게임 엔진을 선택적으로 재구현한 Rust 워크스페이스입니다. [broodwar.live](https://broodwar.live)를 위해 제작되었으며, [OpenBW](https://github.com/broodwar-live/openbw) C++ 엔진을 기반으로 합니다.

**파싱에는 게임 데이터 파일이 필요하지 않습니다.** 파서는 모든 리플레이에서 플레이어 정보, 커맨드, 빌드 오더, APM을 추출합니다. 시뮬레이션 엔진은 BW의 `.dat` 파일이 제공되면 이동 물리, 경로 탐색, 전투, 전장의 안개를 추가합니다.

## 빠른 시작

### Rust

```rust
let replay = replay_core::parse(&std::fs::read("game.rep")?)?;

println!("{} on {}", 
    replay.header.players.iter().map(|p| &p.name).collect::<Vec<_>>().join(" vs "),
    replay.header.map_name
);
println!("{:.0}s, {} commands", replay.header.duration_secs(), replay.commands.len());

for entry in replay.build_order.iter().take(5) {
    println!("  {:.0}s P{} {}", entry.real_seconds, entry.player_id, entry.action);
}
```

### 브라우저

```sh
wasm-pack build crates/replay-wasm --target web --out-dir ../../pkg
python3 -m http.server 8000
# http://localhost:8000/demo/index.html 열기
```

[데모 페이지](demo/index.html)에서 `.rep` 파일과 선택적으로 게임 데이터 파일을 로드하여 시뮬레이션 및 맵 렌더링을 할 수 있습니다.

## 기능

| 기능 | 필요 파일 | 설명 |
|------|----------|------|
| **리플레이 파싱** | `.rep` 파일만 | 헤더, 플레이어, 커맨드, 빌드 오더, APM, 타임라인 |
| **맵 지형** | + CV5, VF4 | 이동 가능 그리드, 높이 맵, 타일셋 식별 |
| **맵 렌더링** | + VX4, VR4, WPE | 미니타일 픽셀 데이터, 팔레트 색상, 타일 그래픽 참조 |
| **유닛 시뮬레이션** | + units.dat, flingy.dat | 이동 물리, 가속, 회전, 웨이포인트 추적 |
| **경로 탐색** | (포함됨) | 타일 수준 A*와 리전 폴백, 대각선 코너 방지 |
| **전투** | + weapons.dat | 지상+공중 무기, 데미지 타입 (진동/폭발/일반 vs 유닛 크기), 프로토스 실드 |
| **기술 & 업그레이드** | + techdata.dat, upgrades.dat | 연구 비용/시간, 업그레이드 레벨 스케일링, 전투 보너스 적용 |
| **생산** | (포함됨) | 빌드 큐, 훈련 타이머, 자원 차감, 서플라이 추적 |
| **건설 & 변태 타이머** | (포함됨) | 건물 건설 시간, 유닛/건물 변태 타이머 |
| **전장의 안개** | (포함됨) | 플레이어별 시야 및 탐사 그리드 |
| **매치업 감지** | (포함됨) | TvZ/PvT 등 자동 감지, 맵 이름 정규화, 승자 감지 |
| **빌드 오더 검색** | (포함됨) | 편집 거리 + LCS 유사도 메트릭, 유사도 순위 |
| **게임 페이즈 감지** | (포함됨) | 오프닝/초반/중반/후반 기술 랜드마크 기반 감지 |
| **스킬 추정** | (포함됨) | EAPM, 핫키, 일관성, 효율성 기반 종합 스킬 점수 |
| **MPQ 아카이브** | `.mpq` 파일 | 게임 데이터 아카이브 및 `.scx`/`.scm` 맵 파일 읽기 |
| **문자열 테이블** | `stat_txt.tbl` | 데이터 기반 유닛/기술/업그레이드 이름 |
| **스프라이트** | `.grp` 파일 | 유닛 및 건물의 RLE 디코딩 프레임 픽셀 데이터 |

### 리플레이 포맷 지원

| 포맷 | 버전 | 압축 |
|------|------|------|
| 레거시 | Pre-1.18 | PKWare DCL Implode |
| 모던 | 1.18 -- 1.20 | zlib |
| 리마스터 | 1.21+ | zlib + 확장 섹션 |

## 크레이트

각 크레이트에는 자체 [`docs/architecture.md`](crates/replay-core/docs/architecture.md)가 있으며 상세한 모듈 맵과 설계 노트가 포함되어 있습니다.

| 크레이트 | 설명 |
|----------|------|
| [`replay-core`](crates/replay-core/) | `.rep` 파일을 구조화된 Rust 타입으로 파싱. 40개 이상의 커맨드 변형, 빌드 오더 추출, APM 분석, 타임라인, 매치업 감지, 빌드 오더 유사도, 게임 페이즈 감지, 스킬 추정. |
| [`bw-engine`](crates/bw-engine/) | 선택적 BW 엔진 재구현. 맵 지형, fp8 물리 기반 유닛 시뮬레이션, 타일 수준 A* 경로 탐색, 데미지 타입과 실드 포함 지상+공중 전투, 자원 추적 생산, 전장의 안개. MPQ 아카이브, SCX/SCM 맵, TBL 문자열 테이블, GRP 스프라이트, 전체 .dat 게임 데이터 파서 포함. 21개 모듈. |
| [`replay-wasm`](crates/replay-wasm/) | wasm-bindgen을 통한 WASM 바인딩. `parseReplay()`, `GameMap`, `GameSim`, `MpqFile`, `ScxMapFile`, `TblFile`, `GrpFile`, `TilesetPalette`, `TilesetVx4`, `TilesetVr4`. |
| [`replay-nif`](crates/replay-nif/) | Rustler를 통한 Elixir NIF 바인딩. `parse_replay`, `parse_header`, `extract_build_order`, `calculate_apm`, `apm_over_time` 5개 NIF 함수. |

## 엔진 아키텍처

시뮬레이션 엔진은 [OpenBW](https://github.com/broodwar-live/openbw) 레퍼런스에서 BW 서브시스템을 재구현합니다:

```
.rep file ──> replay-core ──> commands + map CHK data
                                  │
              units.dat ──────────┤
              flingy.dat ─────────┤
              weapons.dat ────────┤
              techdata.dat ───────┤
              upgrades.dat ───────┤
              orders.dat ─────────┤
              CV5 + VF4 ──────────┤
                                  ▼
                            bw-engine::Game
                                  │
                    ┌─────────────┼──────────────┐
                    │             │              │
                이동          전투/사망        생산
              (fp8 물리,     (지상+공중,     (빌드 큐,
               경로 탐색,     데미지 타입,    자원 비용,
               웨이포인트)     실드)          서플라이 체크)
                    │             │              │
                    └─────────────┼──────────────┘
                                  │
                    ┌─────────────┼──────────────┐
                    │             │              │
              유닛 위치       전장의 안개     플레이어 상태
              (x, y, 타입,   (시야,         (미네랄, 가스,
               소유자, HP,     탐사)          서플라이, 업그레이드,
               실드)                          기술)

파일 포맷 지원:
  .mpq ──> MpqArchive ──> 경로로 파일 추출
  .scx ──> ScxMap ────────> CHK 지형 + 유닛 배치
  .tbl ──> StringTable ───> 인덱스 기반 게임 텍스트
  .grp ──> Grp ───────────> RLE 디코딩 스프라이트 프레임
  VX4/VR4/WPE ────────────> 타일 그래픽 + 팔레트
```

### 주요 설계 결정

- **파일시스템 접근 없음** — 모든 입력은 `&[u8]`이며, 설계부터 WASM 호환
- **고정소수점 연산** — 24.8 `Fp8` 타입으로 BW의 결정론적 물리를 재현
- **태그 호환** — 유닛 슬롯 할당이 BW의 순서와 일치 (터렛 서브유닛, 근접 시작 유닛)
- **2단계 경로 탐색** — 타일 수준 A* (2048 노드 예산)와 리전 그래프 폴백
- **선택적 재구현** — 전체 엔진이 아닌 리플레이 뷰어에 필요한 서브시스템만 구현

## 데모

[`demo/`](demo/) 디렉토리에는 브라우저에서 완전히 실행되는 단일 페이지 리플레이 뷰어가 있습니다:

- `.rep` 파일을 로드하여 리플레이 메타데이터 확인
- `units.dat` + `flingy.dat`를 추가하면 재생/일시정지/속도 조절이 가능한 시뮬레이션 활성화
- `weapons.dat`를 추가하면 전투 활성화
- 타일셋 CV5/VF4 파일을 추가하면 이동 가능 그리드 렌더링
- 유닛 위치가 플레이어별 색상의 점으로 렌더링

게임 데이터 파일은 StarCraft 설치 경로의 `arr/` 및 `tileset/` 디렉토리에 있습니다. 자세한 내용은 [게임 데이터 파일](docs/game-data-files.md)을 참조하세요.

## 테스트

```sh
cargo test --workspace    # 242개 테스트
cargo test -p bw-engine   # 146개 유닛 + 6개 통합 테스트 (실제 리플레이 픽스처)
```

통합 테스트는 5개의 실제 리플레이 픽스처(모던 + 레거시 포맷, 최대 53K 프레임)에 대해 전체 시뮬레이션 파이프라인을 실행하고 89-95% 커맨드 변환 커버리지로 크래시 없는 실행을 검증합니다.

## 라이선스

MIT

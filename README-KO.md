# civitai-cli

ComfyUI 중심 워크플로를 위한 터미널 기반 Civitai 브라우저 및 다운로더입니다.

`civitai-cli`는 다음 기능을 제공합니다.
- 모델 탐색 및 검색
- 모델 북마크
- 이미지 피드 탐색 및 검색
- 이미지 북마크
- 일시정지, 재개, 취소, 히스토리를 포함한 다운로드 관리
- 모델 검색, 이미지 검색, 모델 커버, 이미지 바이트에 대한 영속 캐시
- 로컬 설정 및 캐시 경로 커스터마이징

## 설치

현재 프로젝트 버전: `1.2.0`

이 프로젝트는 Civitai 공개 API와 로컬 ComfyUI 모델 폴더를 기반으로 동작하는 TUI 중심 도구입니다.

### 요구 사항

- Rust stable
- `cargo`
- `ratatui` 기반 TUI를 지원하는 터미널

선택 사항이지만 권장:
- 설정된 ComfyUI 디렉토리
- Civitai API key

### 클론

```bash
git clone https://github.com/Bogyie/civitai-cli.git
cd civitai-cli
```

### 빌드

```bash
cargo build
```

또는:

```bash
make build
```

### 릴리즈 패키지로 설치

Debian / Ubuntu:

```bash
curl -LO https://github.com/Bogyie/civitai-cli/releases/download/v1.2.0/civitai-cli_1.2.0_amd64.deb
sudo dpkg -i civitai-cli_1.2.0_amd64.deb
```

Fedora / RHEL / openSUSE:

```bash
curl -LO https://github.com/Bogyie/civitai-cli/releases/download/v1.2.0/civitai-cli-1.2.0-1.x86_64.rpm
sudo rpm -i civitai-cli-1.2.0-1.x86_64.rpm
```

## 스크린샷

### Models

![Models](assets/screenshots/models.png)

### Model Bookmarks

![Model Bookmarks](assets/screenshots/model-bookmarks.png)

### Image Feed

![Image Feed](assets/screenshots/image-feed.png)

### Image Bookmarks

![Image Bookmarks](assets/screenshots/image-bookmarks.png)

### Downloads

![Downloads](assets/screenshots/downloads.png)

### Settings

![Settings](assets/screenshots/settings.png)

## 주요 기능

### Models

- Civitai 모델 목록 탐색
- 다음 조건으로 검색 가능:
  - 텍스트 쿼리
  - 모델 타입
  - 정렬 기준
  - 베이스 모델
  - 기간
- `nextPage` 기반 무한 스크롤
- 쿼리 단위 결과 캐시
- 북마크된 모델 강조 표시
- 다음 정보 확인 가능:
  - 설명
  - 통계
  - 버전
  - 파일 목록
  - 커버 이미지
  - 메타데이터
- 현재 선택한 모델/버전에 대해 커버 이미지 우선 fetch

### Model Bookmarks

- 모델 목록에서 바로 북마크 추가/해제
- 전용 북마크 탭 제공
- 북마크 검색/필터 지원
- 북마크 import/export 지원
- 북마크 영속 저장

### Image Feed

- TUI에서 Civitai 이미지 피드 탐색
- 다음 조건으로 검색 가능:
  - `nsfw`
  - `sort`
  - `period`
  - `modelVersionId`
  - `tags`
- 태그 텍스트는 쿼리 전에 숫자 tag ID로 변환
- `nextPage` 기반 커서 페이징
- 끝에 가까워지면 자동 prefetch
- API 응답의 video 항목은 자동 스킵
- 한 페이지가 전부 video여도 이미지가 나올 때까지 추가 fetch
- 이미지 패널에서 다음 정보 확인 가능:
  - 렌더된 이미지 미리보기
  - 상세 메타데이터
  - Civitai 이미지 링크

### Image Bookmarks

- 이미지 피드에서 바로 북마크 추가/해제
- 전용 이미지 북마크 탭 제공
- 북마크된 이미지 검색 지원
- 이미지 북마크 영속 저장

### Downloads

- 선택한 모델/버전을 ComfyUI 스타일 폴더에 다운로드
- 베이스 모델과 원본 파일명을 기반으로 스마트 파일명 생성
- 다운로드 일시정지 / 재개 / 취소
- 다운로드 히스토리 탭 제공
- 다음 삭제 동작 지원:
  - 히스토리만 삭제
  - 파일과 히스토리 함께 삭제
- 중단된 다운로드 상태 영속 저장
- 다음 실행 시 중단 다운로드 재개 가능

### Caching

- 모델 검색 캐시:
  - 디스크 영속 저장
  - 쿼리 단위 캐시
  - TTL 설정 가능
- 이미지 검색 캐시:
  - 디스크 영속 저장
  - 기본적으로 짧은 TTL
  - Settings에서 설정 가능
- 모델 커버 캐시:
  - 별도 디렉토리에 저장
- 이미지 바이트 캐시:
  - 별도 디렉토리에 저장
  - 기본값은 영속

### Settings

TUI에서 다음 항목을 설정할 수 있습니다.
- API key
- ComfyUI 경로
- 모델 북마크 경로
- 이미지 북마크 경로
- 모델 검색 캐시 폴더
- 모델 커버 캐시 폴더
- 이미지 캐시 폴더
- 다운로드 히스토리 경로
- 중단 다운로드 히스토리 경로
- 모델 검색 캐시 TTL
- 이미지 검색 캐시 TTL
- 이미지 바이트 캐시 TTL

## 인증

앱은 API key 기반 인증을 지원합니다.

다운로드 요청 시 현재 다음을 함께 사용합니다.
- `Authorization: Bearer <token>`
- `Content-Type: application/json`
- 다운로드 URL의 `token=...` query parameter

이렇게 중복해서 보내는 이유는, 다운로드 엔드포인트에 따라 통과 방식이 다를 수 있기 때문입니다.

## 실행

### TUI 실행

```bash
cargo run
```

또는:

```bash
make run
```

### 디버그 모드 실행

```bash
make run-debug
```

디버그 모드에서는 fetch debug log도 함께 활성화됩니다.

## CLI 사용법

### TUI 열기

```bash
cargo run -- ui
```

### CLI에서 설정 업데이트

```bash
cargo run -- config --api-key YOUR_TOKEN
```

```bash
cargo run -- config --comfyui-path /path/to/ComfyUI
```

### 모델 ID로 다운로드

```bash
cargo run -- download --id 123456
```

### 모델 버전 hash로 다운로드

```bash
cargo run -- download --hash abcdef123456
```

## TUI 메뉴얼

### 탭

현재 탭 구성:
- `1` Models
- `2` Bookmarks
- `3` Image Feed
- `4` Image Bookmarks
- `5` Downloads
- `6` Settings

이동:
- `Tab`: 다음 탭
- 숫자 키: 해당 탭으로 바로 이동

### Models 탭

주요 조작:
- `j` / `k`: 모델 목록 이동
- `h` / `l`: 선택 모델의 버전 이동
- `/`: 모델 검색 폼 열기
- `R`: 현재 모델 쿼리 캐시 무효화 후 새로고침
- `b`: 북마크 토글
- `d`: 선택 모델/버전 다운로드
- `m`: 모달/상세 보기 열기 또는 닫기

확인 가능한 정보:
- 모델 설명
- 버전 목록
- 파일 목록
- 통계
- 모델 커버 이미지
- 메타데이터

### Bookmarks 탭

주요 조작:
- `j` / `k`: 북마크 목록 이동
- `/`: 북마크 검색
- `b`: 북마크 제거
- import/export는 모달에서 경로 입력 방식으로 지원

### Image Feed 탭

주요 조작:
- `j` / `k`: 이미지 이동
- `/`: 이미지 검색 폼 열기
- `b`: 이미지 북마크 토글
- `m`: 모달/상세 보기 열기 또는 닫기

동작 방식:
- 탭 진입 시 피드 로딩 시작
- 배치 단위로 결과 fetch
- 추가 페이지는 `nextPage` 기반으로 로딩
- 현재 위치가 마지막 `5`개 영역에 들어가면 prefetch
- video 항목은 스킵

표시되는 메타데이터:
- image id
- Civitai 링크
- 원본 URL
- hash
- type
- 크기
- NSFW 관련 정보
- browsing level
- 생성 시각
- post id
- username
- base model
- model version ids
- stats
- 존재하는 경우 전체 `meta` JSON

### Image Bookmarks 탭

- 저장된 이미지 탐색
- 북마크 이미지 검색
- `b`로 북마크 제거

### Downloads 탭

주요 조작:
- `p`: 선택 다운로드 일시정지
- `r`: 선택 다운로드 재개
- `c`: 선택 다운로드 취소
- `d`: 히스토리만 삭제
- `D`: 필요 시 취소 후 파일과 히스토리 삭제

추적 정보:
- 현재 다운로드된 용량
- 총 용량
- 진행률
- 상태
- 재실행 후에도 유지되는 히스토리

### Settings 탭

다음 항목을 관리할 수 있습니다.
- API key
- ComfyUI 경로
- 캐시 경로
- 북마크/히스토리 경로
- 검색/이미지 캐시 TTL

## 캐시 및 데이터 저장 위치

캐시 및 영속 파일은 앱 설정 디렉토리 아래에 저장됩니다.

일반적으로 다음이 포함됩니다.
- 모델 검색 캐시 디렉토리
- 이미지 검색 캐시 디렉토리
- 모델 커버 캐시 디렉토리
- 이미지 캐시 디렉토리
- 북마크 파일
- 다운로드 히스토리
- 중단 다운로드 상태
- 디버그 빌드용 fetch 로그

macOS 기본 경로:

```text
~/Library/Application Support/com.civitai/civitai-cli
```

Linux 기본 경로:

```text
~/.config/com.civitai/civitai-cli
```

## Make 타겟

[Makefile](/Users/dev/repo/github/bogyie/civitai-cli/Makefile) 기준:

- `make build`
- `make run`
- `make run-debug`
- `make lint`
- `make fmt`
- `make fetch-log`
- `make tail-fetch-log`
- `make clear-fetch-log`

## Release 흐름

`v*` 형식의 태그를 push하면 GitHub Actions가 자동으로 GitHub Release를 생성합니다.

예시:

```bash
git tag v1.2.0
git push origin v1.2.0
```

릴리즈 workflow는 [Cargo.toml](/Users/dev/repo/github/bogyie/civitai-cli/Cargo.toml)의 버전과 태그 버전이 일치하는지 검증합니다.

## 참고

- 이 프로젝트는 Civitai 공개 REST API와 로컬 ComfyUI 사용을 전제로 합니다.
- Civitai API 응답은 필드 타입이 일정하지 않을 수 있어, 코드에 호환 처리 로직이 포함되어 있습니다.
- 이미지 피드 필터링과 페이징은 upstream API 동작에 영향을 받으므로, 향후 조정이 필요할 수 있습니다.

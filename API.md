# YouTube Download Service API

目标：给其他服务调用的“单请求下载并返回 MP4”接口；请求方只需要把响应保存成 `.mp4`。

## 服务地址

```
http://localhost:8080
```

## 1. 健康检查

```
GET /
```

## 2. 下载并返回文件（核心）

```
POST /download
Content-Type: application/json

{
  "url": "https://www.youtube.com/watch?v=VIDEO_ID",
  "mode": "progressive"   // 可选: "progressive"(默认) | "best"
}
```

说明：
- 这是“一次请求完成下载 + 返回文件”的接口：服务端先完整下载到临时目录，再把最终 `.mp4` 传给请求方（所以请求开始阶段可能会等待一段时间才开始返回数据）。
- 请求方只需要把响应保存成 `.mp4` 即可。
- 连接断开后，服务端会自动终止下载并清理临时文件。
- `mode=progressive` 使用单文件格式（通常更稳，但清晰度可能不如 best）。
- `mode=best` 追求最佳画质（服务端会下载并合并后再传输），需要 `ffmpeg`；可在 `config.toml` 里配置 `ffmpeg_bin`。

## 配置文件

服务默认读取工作目录 `config.toml`，也可用 `--config /path/to/config.toml` 指定。

参考模板：`yt-dlp-service/config.example.toml`

常用配置项（示例）：
```toml
# YouTube 访问需要代理时，推荐显式设置（不要依赖 http_proxy/https_proxy 环境变量）
ytdlp_proxy = "socks5://127.0.0.1:7890"

# mode=best 需要 ffmpeg（用于合并音视频）
ffmpeg_bin = "/opt/homebrew/bin/ffmpeg"
```

启动示例：
```bash
./target/release/yt_dlp_service --config config.toml
```

## 失败响应

当下载失败时，服务会返回 JSON（而不是 MP4），例如：
```json
{
  "error": "yt-dlp exited with error (status=...)",
  "stderr_tail": "..."
}
```

## curl 示例

progressive（更稳）：
```bash
curl -L -X POST http://localhost:8080/download \
  -H "Content-Type: application/json" \
  -d '{"url":"https://www.youtube.com/watch?v=VIDEO_ID","mode":"progressive"}' \
  -o video.mp4
```

best（需要 ffmpeg，画质更高）：
```bash
curl -L -X POST http://localhost:8080/download \
  -H "Content-Type: application/json" \
  -d '{"url":"https://www.youtube.com/watch?v=VIDEO_ID","mode":"best"}' \
  -o video.mp4
```

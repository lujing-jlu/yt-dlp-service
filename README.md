# yt-dlp-service

给其他服务调用的 YouTube 下载服务：单请求下载并返回最终 MP4。

核心接口：`POST /download`，服务端会先完整下载到临时目录，下载成功后再把最终 `.mp4` 返回给请求方；连接断开会终止下载并清理临时文件。

## API

见 `API.md`。

## 配置（config.toml）

服务启动时会读取工作目录 `config.toml`，也可用 `--config /path/to/config.toml` 指定。

参考模板：`config.example.toml`

常见配置项：
- `ytdlp_proxy`：访问 YouTube 需要代理时，推荐显式设置（不要依赖 http_proxy/https_proxy 环境变量）
- `ffmpeg_bin`：`mode=best` 需要 ffmpeg 合并音视频（LaunchAgent 下建议写绝对路径）
- `ytdlp_path`：确保包含 `yt-dlp`、`node`（yt-dlp-ejs），以及可选 `ffmpeg`

## 依赖

- `yt-dlp`（以及需要时的 `yt-dlp-ejs`）
- `node`（用于 JS 签名解密）
- `ffmpeg`（仅 `mode=best` 需要）
- 浏览器 cookies：默认从 `edge` 导出

## 运行

```bash
cp config.example.toml config.toml
# 按需编辑 config.toml（代理/ffmpeg/node PATH 等）

cargo run --release -- --config config.toml
```

## macOS 系统服务（LaunchAgent）

```bash
./scripts/install-macos-service.sh
tail -f /tmp/ytdlp-service.log

./scripts/uninstall-macos-service.sh
```

说明：
- plist 模板：`macos/com.ytdlp.service.plist`（包含 `__BIN__`/`__CONFIG__`/`__WORKDIR__` 占位符）
- `install-macos-service.sh` 会 build release、生成 plist 并安装到 `~/Library/LaunchAgents/`
- LaunchAgent 默认 PATH 很“干净”，请在 `config.toml` 的 `ytdlp_path` 里补齐依赖路径

## 免责声明

请遵守目标网站/平台的服务条款与当地法律法规。本项目仅用于技术研究与自有内容处理。


# YouTube Download Service

流式下载 YouTube 视频服务，边下载边传输，不在本地永久保存。

## 功能

- 同步流式下载：一次请求，实时边下边传
- 连接断开自动停止下载并清理临时文件
- 定时刷新 cookies（从 Edge 导出到 `cookies.txt`）
- 支持并发限制（最多 5 个）

## 快速开始

### macOS 安装

```bash
# 1. 构建发布版本
cargo build --release

# 2. 复制二进制文件到本地 bin 目录
mkdir -p ~/bin
cp target/release/yt_dlp_service ~/bin/

# 3. 创建启动配置文件
cat > ~/Library/LaunchAgents/com.ytdlp.service.plist <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.ytdlp.service</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/jlu/bin/yt_dlp_service</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StdoutPath</key>
    <string>/tmp/ytdlp.log</string>
    <key>StderrPath</key>
    <string>/tmp/ytdlp.error.log</string>
    <key>WorkingDirectory</key>
    <string>/Users/jlu/projects/ytdownload/yt-dlp-service</string>
</dict>
</plist>
EOF

# 4. 加载并启动服务
launchctl load ~/Library/LaunchAgents/com.ytdlp.service.plist
launchctl start com.ytdlp.service

# 5. 验证服务
curl http://localhost:8080
```

### 服务管理

```bash
# 查看服务状态
launchctl list | grep ytdlp

# 停止服务
launchctl stop com.ytdlp.service

# 启动服务
launchctl start com.ytdlp.service

# 重启服务
launchctl stop com.ytdlp.service && launchctl start com.ytdlp.service

# 取消开机自启
launchctl unload ~/Library/LaunchAgents/com.ytdlp.service.plist

# 查看日志
tail -f /tmp/ytdlp.log
```

### Ubuntu/Debian 安装

```bash
# 1. 构建发布版本
cargo build --release

# 2. 复制二进制文件
sudo cp target/release/yt_dlp_service /usr/local/bin/

# 3. 创建 systemd 服务
sudo cat > /etc/systemd/system/ytdlp.service <<'EOF'
[Unit]
Description=YouTube Download Service
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/yt_dlp_service
WorkingDirectory=/path/to/yt-dlp-service
Restart=on-failure
StandardOutput=append:/var/log/ytdlp.log
StandardError=append:/var/log/ytdlp.error.log

[Install]
WantedBy=multi-user.target
EOF

# 4. 启动服务
sudo systemctl daemon-reload
sudo systemctl enable ytdlp
sudo systemctl start ytdlp

# 5. 验证服务
curl http://localhost:8080
```

### 直接运行

```bash
# 开发模式
cargo run

# 或发布版本
./target/release/yt_dlp_service
```

---

## API 接口

### 1. 健康检查

```
GET /
```

### 2. 同步流式下载（核心）

```
POST /download
Content-Type: application/json

{
  "url": "https://www.youtube.com/watch?v=VIDEO_ID",
  "mode": "progressive"   // 可选: "progressive"(默认) | "best"
}
```

说明：
- 这是“一次请求完成下载 + 返回文件”的接口；服务端先完整下载，再把最终 `.mp4` 传给请求方。
- 请求方只需保存成 `.mp4`。
- 连接断开后，服务端会自动终止下载并清理临时文件。
 - `mode=best` 需要 `ffmpeg`（用于合并音视频），可在 `config.toml` 里设置 `ffmpeg_bin`。

---

## 使用示例

```bash
curl -L -X POST http://localhost:8080/download \
  -H "Content-Type: application/json" \
  -d '{"url":"https://www.youtube.com/watch?v=VIDEO_ID","mode":"progressive"}' \
  -o video.mp4
```

---

## 配置文件（config.toml）

服务启动时会读取工作目录下的 `config.toml`，也可以用 `--config /path/to/config.toml` 指定路径。

参考模板：`yt-dlp-service/config.example.toml`

---

## 源码结构

- `yt-dlp-service/src/main.rs`：HTTP 服务启动、并发限制、后台 cookies 刷新
- `yt-dlp-service/src/handlers.rs`：`/`、`/download` 处理逻辑
- `yt-dlp-service/src/cookies.rs`：cookies 刷新/检查
- `yt-dlp-service/src/config.rs`：配置文件读取与默认值
- `yt-dlp-service/src/state.rs`：共享状态（Semaphore、cookies lock）
- `yt-dlp-service/src/util.rs`：小工具（文件名/视频 ID 解析）

## 前置条件

- Rust 1.70+
- yt-dlp: `pip install yt-dlp yt-dlp-ejs`
- Node.js (用于 JS 签名解密)
- Edge 浏览器已登录 YouTube (用于获取 cookies)

---

## macOS 系统服务（LaunchAgent）

项目提供了 LaunchAgent 模板与安装脚本：
- `yt-dlp-service/macos/com.ytdlp.service.plist`（按需修改里面的路径）
- `yt-dlp-service/scripts/install-macos-service.sh`
- `yt-dlp-service/scripts/uninstall-macos-service.sh`

安装/启动：
```bash
./scripts/install-macos-service.sh
```

停止/卸载：
```bash
./scripts/uninstall-macos-service.sh
```

查看日志：
```bash
tail -f /tmp/ytdlp-service.log
tail -f /tmp/ytdlp-service.error.log
```

---

## Docker 部署

```dockerfile
FROM rust:1.70 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM ubuntu:22.04
RUN apt-get update && apt-get install -y \
    python3 \
    nodejs \
    ffmpeg \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/yt_dlp_service /app/
EXPOSE 8080
CMD ["/app/yt_dlp_service"]
```

---

## 注意事项

1. 需要 Edge 浏览器已登录 YouTube (cookies 有效期约 12 小时)
2. 需要 Node.js 环境处理签名解密
3. 视频下载完成后保存在临时目录，流式传输完成后自动清理
4. 建议设置超时时间较长的反向代理 (如 nginx)
5. 最大支持 5 个并发下载
6. macOS 需要在终端中授权 Full Disk Access 给 `yt_dlp_service` (如遇权限问题)

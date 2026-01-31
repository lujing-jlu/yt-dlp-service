#!/bin/bash
# YouTube Download Service 客户端脚本
# 用法: ./download.sh <youtube_url> [output_name] [mode]

set -e

SERVER="${YT_DLP_SERVER:-http://127.0.0.1:8080}"
URL="$1"
OUTPUT_NAME="$2"
MODE="${3:-${YT_DLP_MODE:-progressive}}"

usage() {
    echo "用法: $0 <youtube_url> [output_name] [mode]"
    echo ""
    echo "环境变量:"
    echo "  YT_DLP_SERVER  服务地址 (默认: http://127.0.0.1:8080)"
    echo "  YT_DLP_MODE    下载模式 (默认: progressive)"
    echo ""
    echo "示例:"
    echo "  $0 https://www.youtube.com/watch?v=U3yt2l3pOsE"
    echo "  $0 https://www.youtube.com/watch?v=U3yt2l3pOsE myvideo"
    echo "  $0 https://www.youtube.com/watch?v=U3yt2l3pOsE myvideo best"
    exit 1
}

log() {
    echo "[$(date '+%H:%M:%S')] $1"
}

# 检查参数
if [ -z "$URL" ]; then
    usage
fi

# 生成文件名
if [ -z "$OUTPUT_NAME" ]; then
    OUTPUT_NAME="video_$(date +%s)"
fi

log "开始下载: $URL"
log "服务: $SERVER"
log "模式: $MODE"

VIDEO_FILE="${OUTPUT_NAME}.mp4"
TMP_FILE="${VIDEO_FILE}.part"

log "开始同步下载 (边下边传)..."
# 强制客户端请求不走代理（常见场景：http_proxy 指向本机 127.0.0.1:7890，但代理未启动/不支持内网地址）
env -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u no_proxy -u NO_PROXY \
curl --noproxy '*' -L --progress-bar -X POST "$SERVER/download" \
    -H "Content-Type: application/json" \
    -d "{\"url\":\"$URL\",\"mode\":\"$MODE\"}" \
    -o "$TMP_FILE"

if [ ! -s "$TMP_FILE" ]; then
    log "错误: 视频下载失败或为空"
    exit 1
fi

mv -f "$TMP_FILE" "$VIDEO_FILE"

VIDEO_SIZE=$(ls -lh "$VIDEO_FILE" | awk '{print $5}')
log "视频下载完成: $VIDEO_FILE ($VIDEO_SIZE)"

echo ""
echo "========================================"
echo "  下载完成!"
echo "  视频: $VIDEO_FILE"
echo "========================================"

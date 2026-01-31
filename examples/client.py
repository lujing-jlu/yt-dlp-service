#!/usr/bin/env python3
"""
YouTube Download Service Python Client
Usage: python client.py <youtube_url> [output_name] [mode]
"""

import os
import sys
import requests

SERVER_URL = os.environ.get("YT_DLP_SERVER_URL", "http://localhost:8080")


def download_video(url: str, output_name: str, mode: str) -> str:
    tmp = output_name + ".mp4.part"
    final = output_name + ".mp4"

    s = requests.Session()
    # Don't inherit http_proxy/https_proxy from environment; clients should talk to the service directly.
    s.trust_env = False
    with s.post(
        f"{SERVER_URL}/download",
        json={"url": url, "mode": mode},
        stream=True,
        timeout=None,
    ) as r:
        r.raise_for_status()
        with open(tmp, "wb") as f:
            for chunk in r.iter_content(chunk_size=64 * 1024):
                if chunk:
                    f.write(chunk)

    os.replace(tmp, final)
    return final


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: python client.py <youtube_url> [output_name] [mode]")
        print("\nEnvironment variables:")
        print("  YT_DLP_SERVER_URL - Server URL (default: http://localhost:8080)")
        sys.exit(1)

    url = sys.argv[1]
    output = sys.argv[2] if len(sys.argv) >= 3 else "video"
    mode = sys.argv[3] if len(sys.argv) >= 4 else "progressive"

    print(f"Server: {SERVER_URL}")
    print(f"URL: {url}")
    print(f"Mode: {mode}")
    print(f"Output: {output}.mp4")

    filename = download_video(url, output, mode)
    print(f"Done: {filename}")


if __name__ == "__main__":
    main()

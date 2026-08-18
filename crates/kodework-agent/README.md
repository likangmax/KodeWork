# kodework-agent

`kodework-agent` 是可选的远程后台执行组件。它只监听 loopback，建议通过 SSH 本地端口转发访问，不应直接暴露到公网。

## 启动

令牌必须至少 32 个字符，且不能包含空白、控制字符或换行以外的内容。推荐使用权限为仅运行用户可读的令牌文件：

```text
kodework-agent.exe --port 43127 --token-file C:\Users\user\.config\kodework\agent.token
```

也可使用环境变量 `KODEWORK_AGENT_TOKEN`。命令行不接受 token 参数，避免令牌出现在进程列表、崩溃报告或 shell 历史中。

## 协议与边界

- 每个 TCP 连接只处理一个 JSON 请求。
- 首行是 token，第二行是请求；协议读取 10 秒超时。
- 单行请求最大 1 MiB，单次输出最大 256 KiB；执行超时限制为 1–3600 秒。
- 同时最多 32 个连接；超出连接直接关闭。
- 支持 `status`、`exec`、`tmux_sessions` 请求。
- agent 不负责 SSH 身份认证、Tailscale 登录或公网访问控制；这些由 Kodework 主程序和 SSH 隧道负责。


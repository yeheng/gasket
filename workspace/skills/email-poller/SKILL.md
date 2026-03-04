---
name: email-poller
description: Outlook email polling via EWS with NTLM authentication (cross-platform)
always: false
bins:
  - python3
env_vars:
  - OUTLOOK_EWS_URL
  - OUTLOOK_USERNAME
  - OUTLOOK_PASSWORD
  - OUTLOOK_DOMAIN
---

# Outlook Email Poller Skill

通过 Exchange Web Services (EWS) 访问 Outlook 2010+ 邮件，支持 NTLM 认证，兼容 macOS 和 Windows。

## 架构图

```
┌─────────────────────────────────────────────┐
│  Email Poller Scripts                       │
│  ┌─────────┐ ┌─────────┐ ┌─────────────┐   │
│  │ fetch   │ │ search  │ │ poll_new    │   │
│  │ get     │ │ mark    │ │ get_unread  │   │
│  └─────────┘ └─────────┘ └─────────────┘   │
├─────────────────────────────────────────────┤
│  Python + requests-ntlm (macOS)             │
│  PowerShell/.NET (Windows)                  │
├─────────────────────────────────────────────┤
│  NTLM Authentication                        │
├─────────────────────────────────────────────┤
│  EWS SOAP Client                            │
└─────────────────────────────────────────────┘
                    │
                    ▼
        ┌───────────────────────┐
        │  Exchange Server 2010+│
        └───────────────────────┘
```

## Prerequisites

### 环境依赖

| 平台 | 依赖 | 安装 |
|------|------|------|
| macOS | Python 3.8+ | `brew install python@3.11` |
| macOS | requests-ntlm | `pip3 install requests-ntlm lxml` |
| Windows | PowerShell 5.1+ | 系统自带 |

### 环境变量

```bash
export OUTLOOK_EWS_URL="https://outlook.company.com/EWS/Exchange.asmx"
export OUTLOOK_USERNAME="your.username"
export OUTLOOK_PASSWORD="your.password"
export OUTLOOK_DOMAIN="COMPANY"
```

## Scripts

所有脚本位于 `workspace/skills/email-poller/scripts/` 目录。

### 1. fetch_emails.py - 分页获取邮件

```bash
# 获取最新 50 封邮件
python3 scripts/fetch_emails.py --page-size 50 --offset 0

# JSON 输出
python3 scripts/fetch_emails.py --page-size 20 --json

# 获取第 2 页（51-100）
python3 scripts/fetch_emails.py --page-size 50 --offset 50
```

**参数:**
- `--page-size N` - 每页数量（默认 50）
- `--offset N` - 偏移量（默认 0）
- `--json` - JSON 格式输出

**输出字段:** subject, sender_name, sender_email, received_time, is_read, item_id, change_key

---

### 2. get_email_details.py - 获取邮件详情

```bash
# 获取完整邮件（含 body）
python3 scripts/get_email_details.py <item_id> <change_key>

# 仅输出 body
python3 scripts/get_email_details.py <item_id> <change_key> --body-only

# JSON 输出
python3 scripts/get_email_details.py <item_id> <change_key> --json
```

**参数:**
- `item_id` - 邮件 ItemId（从 fetch_emails 获取）
- `change_key` - 邮件 ChangeKey
- `--body-only` - 仅输出 body 内容
- `--json` - JSON 格式输出

**输出字段:** subject, body, body_type, sender, to_recipients, cc_recipients, received_time

---

### 3. search_emails.py - 搜索邮件

```bash
# 搜索主题包含"报告"的邮件
python3 scripts/search_emails.py --subject "报告"

# 搜索特定发件人
python3 scripts/search_emails.py --from "boss@company.com"

# 最近 30 天的邮件
python3 scripts/search_emails.py --days-back 30

# 组合条件
python3 scripts/search_emails.py --subject "周报" --days-back 7 --page-size 20
```

**参数:**
- `--subject KEYWORD` - 主题关键字
- `--from ADDRESS` - 发件人地址
- `--days-back N` - 搜索最近 N 天（默认 7）
- `--page-size N` - 返回数量（默认 50）
- `--json` - JSON 格式输出

---

### 4. poll_new_emails.py - 轮询新邮件

```bash
# 检查新邮件（自动记录上次阅读时间）
python3 scripts/poll_new_emails.py

# 默认检查最近 2 小时（无状态文件时）
python3 scripts/poll_new_emails.py --hours-back 2

# JSON 输出
python3 scripts/poll_new_emails.py --json
```

**参数:**
- `--state-file PATH` - 状态文件路径（默认 ~/.nanobot/email-poller-state.json）
- `--hours-back N` - 默认检查 N 小时（默认 1）
- `--json` - JSON 格式输出

---

### 5. get_unread_emails.py - 获取未读邮件

```bash
# 获取未读邮件
python3 scripts/get_unread_emails.py --page-size 20

# 获取并标记为已读
python3 scripts/get_unread_emails.py --page-size 20 --mark-as-read

# JSON 输出
python3 scripts/get_unread_emails.py --json
```

**参数:**
- `--page-size N` - 获取数量（默认 20）
- `--mark-as-read` - 标记为已读
- `--json` - JSON 格式输出

---

### 6. mark_as_read.py - 标记邮件为已读

```bash
python3 scripts/mark_as_read.py <item_id> <change_key>
```

---

## Common Patterns

### 定时轮询（cron）

```bash
# crontab -e
# 每 5 分钟检查新邮件
*/5 * * * * python3 /path/to/scripts/poll_new_emails.py --json >> /tmp/email-poll.log 2>&1
```

### 获取最新未读邮件并读取

```bash
# 1. 获取未读邮件列表
UNREAD=$(python3 scripts/get_unread_emails.py --page-size 5 --json)

# 2. 解析第一封邮件的 item_id
ITEM_ID=$(echo $UNREAD | jq -r '.emails[0].item_id')
CHANGE_KEY=$(echo $UNREAD | jq -r '.emails[0].change_key')

# 3. 获取完整内容
python3 scripts/get_email_details.py $ITEM_ID $CHANGE_KEY
```

### 搜索并标记已读

```bash
# 搜索最近 7 天主题包含"通知"的邮件并标记已读
python3 scripts/search_emails.py --subject "通知" --days-back 7 --json | \
  jq -r '.[] | "\(.item_id) \(.change_key)"' | \
  while read item_id change_key; do
    python3 scripts/mark_as_read.py "$item_id" "$change_key"
  done
```

## Best Practices

1. **使用虚拟环境** (macOS)
   ```bash
   python3 -m venv ~/.nanobot/email-poller-venv
   source ~/.nanobot/email-poller-venv/bin/activate
   ```

2. **安全存储密码** - 使用 1Password 或系统密钥链

3. **分页获取** - 避免一次性获取大量邮件

4. **错误处理** - 检查脚本返回状态码

## Troubleshooting

| 问题 | 解决 |
|------|------|
| 401 Unauthorized | 检查域名格式：`DOMAIN\username` |
| SSL 错误 | 联系 IT 获取 CA 证书 |
| 连接超时 | 检查 EWS URL 和网络 |
| XML 解析错误 | 确保安装 `lxml` |

## EWS URL 发现

常见 EWS 端点：
- `https://outlook.company.com/EWS/Exchange.asmx`
- `https://mail.company.com/EWS/Exchange.asmx`
- `https://exchange.company.com/EWS/Exchange.asmx`

联系 IT 部门获取正确的 EWS URL。

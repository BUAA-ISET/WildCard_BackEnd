# API 清单

本文档列出系统中所有规划的 API 端点，包括已实现和待实现的接口。

---

## 一、用户系统

| URL | Method | 描述 | 前端配置 | 后端实现 |
|-----|--------|------|----------|----------|
| `/api/user/register` | POST | 用户注册 | ✅ | ✅ 已实现 |
| `/api/user/login` | POST | 用户登录 | ✅ | ✅ 已实现 |
| `/api/user/logout` | GET | 用户登出 | ✅ | ✅ 已实现 |
| `/api/user/find?user_name=xxx` | GET | 根据用户名查找用户 | ❌ | ✅ 已实现 |
| `/api/user/me` | GET | 获取当前登录用户信息 | ❌ | ✅ 已实现 |
| `/api/user/current` | GET | 获取当前登录用户信息 | ✅ | ⏳ 待实现 |
| `/api/user/send-code` | POST | 发送邮箱验证码 | ✅ | ⏳ 待实现 |
| `/api/user/username` | PUT | 修改用户名 | ✅ | ⏳ 待实现 |
| `/api/user/password` | PUT | 修改密码 | ✅ | ⏳ 待实现 |
| `/api/user/avatar` | PUT | 更新头像 | ✅ | ⏳ 待实现 |

---

## 二、规则系统

| URL | Method | 描述 | 状态 |
|-----|--------|------|------|
| `/api/rules/drafts` | POST | 保存规则草稿 | ⏳ 待实现 |
| `/api/rules/drafts/:draftId` | GET | 获取规则草稿详情 | ⏳ 待实现 |
| `/api/rules/drafts/:draftId` | PUT | 更新规则草稿 | ⏳ 待实现 |
| `/api/rules/drafts/:draftId/publish` | POST | 发布规则 | ⏳ 待实现 |
| `/api/room/rules` | GET | 获取可创建房间的规则列表 | ⏳ 待实现 |

---

## 三、房间系统

| URL | Method | 描述 | 前端配置 | 后端实现 |
|-----|--------|------|----------|----------|
| `/api/room/create` | POST | 创建房间 | ✅ | ⏳ 待实现 |
| `/api/room/rules` | GET | 获取可创建房间的规则列表 | ✅ | ⏳ 待实现 |
| `/api/room/join` | POST | 加入房间 | ✅ | ⏳ 待实现 |
| `/api/room/check-password?code=xxx` | GET | 检查房间是否有密码 | ✅ | ⏳ 待实现 |
| `/api/room/current` | GET | 获取当前用户所在房间 | ✅ | ⏳ 待实现 |
| `/api/room/current?code=xxx` | GET | 根据房间码获取房间信息 | ✅ | ⏳ 待实现 |
| `/api/room/current/ready` | POST | 设置准备/取消准备状态 | ✅ | ⏳ 待实现 |
| `/api/room/current/start` | POST | 开始游戏 | ✅ | ⏳ 待实现 |
| `/api/room/leave` | POST | 离开房间 | ✅ | ⏳ 待实现 |

---

## 四、游戏对局

| URL | Method | 描述 | 状态 |
|-----|--------|------|------|
| `/api/games/current?roomCode=xxx` | GET | 获取当前对局快照 | ⏳ 待实现 |
| `/api/games/:sessionId` | GET | 根据会话 ID 获取对局快照 | ⏳ 待实现 |
| `/api/games/:sessionId/actions/:actionId/play-cards` | POST | 出牌 | ⏳ 待实现 |
| `/api/games/:sessionId/actions/:actionId/skip` | POST | 跳过 | ⏳ 待实现 |
| `/api/games/:sessionId/actions/:actionId/choose` | POST | 选择动作选项 | ⏳ 待实现 |

---

## 五、WebSocket 事件

### 服务端 → 客户端

| 事件名 | 描述 |
|--------|------|
| `room.snapshot` | 订阅房间后返回完整快照 |
| `room.updated` | 房间状态更新（玩家加入/离开/准备） |
| `game.started` | 游戏开始通知 |
| `game.snapshot` | 对局完整快照 |
| `game.updated` | 对局状态增量更新 |
| `game.pending_action` | 请求玩家出牌或选择动作 |
| `game.action_rejected` | 玩家动作被拒绝 |
| `game.finished` | 对局结算完成 |
| `error` | 通用错误 |

### 客户端 → 服务端

| 事件名 | 描述 |
|--------|------|
| `room.subscribe` | 订阅房间频道 |
| `room.unsubscribe` | 取消订阅房间 |
| `game.subscribe` | 订阅对局频道 |
| `game.heartbeat` | 心跳检测 |

---

## 六、前端后端对接注意事项

### 1. 用户信息接口路径差异
- **前端期望**: `/api/user/current`
- **后端实现**: `/api/user/me`
- **建议**: 后端增加 `/api/user/current` 作为 `/api/user/me` 的别名，保持兼容性

### 2. 后端额外接口
- `/api/user/find` 已实现，但前端未配置使用

### 3. 前端额外配置
- `/api/user/send-code`、`/api/user/username`、`/api/user/password`、`/api/user/avatar` 前端已配置，后端待实现

---

## 七、响应格式

### 成功响应
```json
{
  "success": true,
  "data": {},
  "message": ""
}
```

### 失败响应
```json
{
  "success": false,
  "message": "错误描述"
}
```

---

## 七、HTTP 状态码

| 状态码 | 含义 |
|--------|------|
| 200 | 成功 |
| 400 | 请求参数错误 |
| 401 | 未授权（登录认证失败） |
| 403 | 禁止访问（权限不足） |
| 404 | 资源不存在 |
| 409 | 冲突（如用户名已存在） |
| 500 | 服务器内部错误 |

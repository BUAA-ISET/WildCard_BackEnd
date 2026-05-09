# Alpha 阶段前后端对接设计

本文档面向当前前端项目的后续对接工作，重点覆盖规则构建、房间准备和游戏对局。用户系统已有基础接口雏形，规则市场在 alpha 阶段暂不实现，只保留规则选择所需的最小列表能力。

## 当前前端现状

前端当前的 API 入口在 `src/api`：

- `userApi`：登录、注册、验证码、当前用户、修改资料等已有 mock/接口路径雏形。
- `roomApi`：当前仍以 localStorage mock 房间为主，支持规则选项、创建房间、加入房间、准备、开始、离开。
- `ruleApi`：目前只把规则构建器导出的 JSON 保存到 localStorage。
- `BattleView.vue`：当前是静态演示界面，尚未接入真实房间、手牌、轮次、出牌请求和服务端事件。

规则构建器导出的运行时规则格式来自 `src/types/ruleBuilder.ts` 的 `ExportedRuleDesign`，生成逻辑在 `src/utils/ruleBuilder.ts` 的 `exportRuleDesign`。后端应优先接住这个导出格式，而不是要求前端重新组织报文。

## 对接目标

alpha 阶段建议先完成以下闭环：

1. 用户登录后可以保存规则草稿，并把草稿发布成“可创建房间的规则”。
2. 创建房间时可以选择自己可用的规则。
3. 玩家可以通过房间号加入房间，准备，房主开始游戏。
4. 游戏开始后，后端作为规则状态机执行 `match_flow`、牌型流程和结算流程。
5. 前端通过 WebSocket 接收房间和对局状态，遇到“出牌组件”或“动作组件”时提交玩家输入。

不在 alpha 阶段做：

- 规则市场、规则搜索、规则评分、规则购买、公开规则详情页。
- 完整历史战绩和回放。
- 复杂观战、断线重连后的完整回放补帧。

## 通用约定

### 基础地址

前端目前配置为：

```ts
BASE_URL = 'http://localhost:3000'
```

建议 HTTP 接口统一以 `/api` 开头，实时对局使用同域 WebSocket：

```text
HTTP:      http://localhost:3000/api/...
WebSocket: ws://localhost:3000/ws
```

### 认证

当前 `apiRequest` 使用：

```ts
credentials: 'include'
```

所以后端建议使用 HttpOnly Cookie 保存会话。所有需要登录的接口通过 Cookie 识别用户。alpha 阶段如暂不做强鉴权，至少要保证房间操作能识别当前玩家身份。

### 响应格式

保持前端现有习惯，统一返回：

```json
{
  "success": true,
  "data": {},
  "message": ""
}
```

失败时：

```json
{
  "success": false,
  "message": "Room does not exist."
}
```

HTTP 状态码仍应正确表达错误类型，例如 `400`、`401`、`403`、`404`、`409`、`422`、`500`。前端会优先读取 `message`。

## 规则对接设计

### 前端导出的规则 JSON

后端保存和执行的核心字段应直接采用 `ExportedRuleDesign`：

```ts
type ExportedRuleDesign = {
  classes: Record<string, {
    default_properties: ExportedPropertyMap
    user_properties: ExportedPropertyMap
    methods: ExportedMethodMap
  }>
  cardsets: Record<string, {
    name: string
    properties: ExportedPropertyMap
    build_flow: Record<string, ExportedFlowNode>
    compare_flow: Record<string, ExportedFlowNode>
    successors: string[]
  }>
  cardset_comparisons: Record<string, {
    cardsetA: string
    cardsetB: string
    compare_flow: Record<string, ExportedFlowNode>
  }>
  match_flow: Record<string, ExportedFlowNode>
  end_flow: Record<string, ExportedFlowNode>
}
```

注意：

- 每个流程图编号 `"1"` 一定是开始节点。
- `content.next`、`content.next_true`、`content.next_false` 等字段已经由前端导出为编号字符串。
- `type` 是组件编号，编号含义参考 `docs/API_JSON格式.md` 和 `docs/TODO.md`。
- `cardset_comparisons` 是当前前端新增字段，建议后端支持。它描述两种牌型之间更细的比较流程，和 `cardsets[*].successors` 的优先级关系互补。

### 规则元数据

前端构建器中有但当前导出 JSON 没包含的元数据：

```ts
{
  "name": "未命名规则",
  "playerCount": 4,
  "description": ""
}
```

保存接口应把这些作为外层字段传给后端，避免只保存运行时 JSON 后丢失列表展示信息。

### 规则状态

建议规则分为两层：

- `draft`：草稿，可反复编辑，不出现在创建房间的规则列表中。
- `published`：已发布，可创建房间。alpha 阶段不需要市场，只需要当前用户可用规则。

推荐状态：

```text
draft -> published -> archived
```

### 规则接口

#### 保存规则草稿

```http
POST /api/rules/drafts
```

请求：

```json
{
  "name": "斗地主",
  "playerCount": 3,
  "description": "三人斗地主规则",
  "design": {
    "classes": {},
    "cardsets": {},
    "cardset_comparisons": {},
    "match_flow": {},
    "end_flow": {}
  }
}
```

响应：

```json
{
  "success": true,
  "data": {
    "id": "rule_draft_001",
    "status": "draft",
    "updatedAt": 1710000000000
  }
}
```

#### 更新规则草稿

```http
PUT /api/rules/drafts/:draftId
```

请求体同保存草稿。后端应校验当前用户是否为作者。

#### 获取规则草稿详情

```http
GET /api/rules/drafts/:draftId
```

响应：

```json
{
  "success": true,
  "data": {
    "id": "rule_draft_001",
    "name": "斗地主",
    "playerCount": 3,
    "description": "三人斗地主规则",
    "status": "draft",
    "design": {},
    "createdAt": 1710000000000,
    "updatedAt": 1710000000000
  }
}
```

#### 发布规则

```http
POST /api/rules/drafts/:draftId/publish
```

后端应在发布时做比前端更严格的校验：

- 必须有 `classes.player`、`classes.card`、`classes.table`。
- 至少一个 `cardsets`。
- `match_flow` 和 `end_flow` 必须有开始节点。
- 所有 `next`、`next_true`、`next_false` 指向存在的流程节点。
- 组件 `type` 必须是后端支持的编号。
- 属性、方法、参数不能重名。
- 流程中引用的对象、属性、方法、牌型名称必须可解析。

响应：

```json
{
  "success": true,
  "data": {
    "ruleId": "rule_001",
    "version": 1,
    "status": "published"
  }
}
```

#### 创建房间可选规则列表

当前前端已有 `roomApi.getRuleOptions()`，路径是 `/api/room/rules`。alpha 阶段可以继续使用这个路径，也可以迁移到更清晰的 `/api/rules/options`。为减少前端改动，建议后端先兼容：

```http
GET /api/room/rules
```

响应：

```json
{
  "success": true,
  "data": [
    {
      "id": "rule_001",
      "name": "斗地主",
      "playerCount": 3,
      "description": "三人斗地主规则"
    }
  ]
}
```

## 房间对接设计

### 房间模型

保持当前前端 `Room` 类型：

```ts
type Room = {
  id: string
  code: string
  hostId: string
  playerCount: number
  roundTime: number
  ruleId: string
  ruleName: string
  password: string | null
  players: Player[]
  status: 'waiting' | 'playing' | 'finished'
}
```

建议后端不要把明文密码返回给前端。为了兼容当前类型，可以返回 `password: null`，另用 `hasPassword` 表示是否有密码。

推荐扩展：

```json
{
  "id": "room_001",
  "code": "A12BCD",
  "hostId": "user_001",
  "playerCount": 3,
  "roundTime": 30,
  "ruleId": "rule_001",
  "ruleName": "斗地主",
  "password": null,
  "hasPassword": true,
  "players": [],
  "status": "waiting",
  "createdAt": 1710000000000
}
```

### 玩家模型

```ts
type Player = {
  id: string
  username: string
  avatar: string
  isReady: boolean
  joinedAt?: number
}
```

后端需要保证：

- 房主默认 `isReady: true`。
- 普通玩家加入后默认 `isReady: false`。
- 房主离开等待房时，应转移房主给最早加入的玩家，并让新房主 ready。
- 如果游戏中有人离开，alpha 阶段建议直接结束对局，结果记为平局或异常结束。

### 房间接口

#### 创建房间

```http
POST /api/room/create
```

请求：

```json
{
  "ruleId": "rule_001",
  "roundTime": 30,
  "password": "abc123"
}
```

响应：

```json
{
  "success": true,
  "data": {
    "id": "room_001",
    "code": "A12BCD",
    "hostId": "user_001",
    "playerCount": 3,
    "roundTime": 30,
    "ruleId": "rule_001",
    "ruleName": "斗地主",
    "password": null,
    "hasPassword": true,
    "players": [
      {
        "id": "user_001",
        "username": "Alice",
        "avatar": "",
        "isReady": true,
        "joinedAt": 1710000000000
      }
    ],
    "status": "waiting"
  }
}
```

#### 检查房间密码

```http
GET /api/room/check-password?code=A12BCD
```

响应：

```json
{
  "success": true,
  "hasPassword": true
}
```

#### 加入房间

```http
POST /api/room/join
```

请求：

```json
{
  "code": "A12BCD",
  "password": "abc123"
}
```

常见失败：

- 房间不存在：`404`
- 密码错误：`403`
- 房间已满：`409`
- 房间已经开始：`409`

#### 获取当前房间

当前前端有两种用法：

```http
GET /api/room/current
GET /api/room/current?code=A12BCD
```

建议后端都支持。带 `code` 时按房间号查询，不带时返回当前用户所在房间。

#### 准备/取消准备

当前前端路径：

```http
POST /api/room/current/ready
```

请求：

```json
{
  "isReady": true
}
```

响应返回最新 `Room`。

#### 开始游戏

当前前端路径：

```http
POST /api/room/current/start
```

后端校验：

- 当前用户必须是房主。
- 房间必须处于 `waiting`。
- 人数必须等于规则要求的 `playerCount`。
- 所有玩家必须 ready。
- 规则必须是已发布且后端可执行。

成功后：

- 房间状态变为 `playing`。
- 创建一局 `gameSession`。
- 初始化玩家、牌桌、卡牌池等运行时对象。
- 通过 WebSocket 广播 `room.updated` 和 `game.started`。

#### 离开房间

```http
POST /api/room/leave
```

响应：

```json
{
  "success": true
}
```

## 游戏对局对接设计

### 为什么需要 WebSocket

准备房目前用轮询和 storage mock。真实对局中存在以下实时事件：

- 玩家加入、离开、准备状态变化。
- 房主开始游戏。
- 后端执行规则流程后更新手牌、牌桌、当前玩家。
- 出牌组件或动作组件要求某个玩家输入。
- 输入超时、无效输入、结算结束。

因此建议 alpha 阶段引入 WebSocket。项目已安装 `socket.io-client`，后端可以使用 Socket.IO，也可以使用原生 WebSocket；为了减少前端集成成本，建议用 Socket.IO。

### 连接流程

客户端连接：

```ts
io(BASE_URL, {
  withCredentials: true,
  query: { roomCode }
})
```

连接后加入房间频道：

```json
{
  "event": "room.subscribe",
  "payload": {
    "code": "A12BCD"
  }
}
```

服务端返回当前快照：

```json
{
  "event": "room.snapshot",
  "payload": {
    "room": {},
    "game": null
  }
}
```

### 对局快照模型

`BattleView` 需要从静态数据切到后端快照。建议后端给每个玩家返回“按权限过滤”的视图：

```ts
type GameSnapshot = {
  sessionId: string
  roomCode: string
  ruleId: string
  status: 'playing' | 'settling' | 'finished'
  currentPlayerId: string
  roundTime: number
  deadlineAt: number | null
  players: GamePlayerView[]
  table: GameTableView
  handCards: GameCard[]
  pendingAction: PendingAction | null
  lastAction: GameActionRecord | null
}
```

玩家视图：

```json
{
  "id": "user_001",
  "username": "Alice",
  "avatar": "",
  "cardCount": 17,
  "publicProperties": {
    "身份": 1,
    "分数": 0
  },
  "online": true
}
```

卡牌视图：

```json
{
  "id": "card_001",
  "properties": {
    "点数": 3,
    "花色": 0
  },
  "display": {
    "rank": "3",
    "suit": "黑桃"
  }
}
```

`display` 字段建议由后端根据规则枚举补充，方便前端直接展示；同时保留 `properties` 供规则判断和调试。

牌桌视图：

```json
{
  "playedCards": [
    {
      "id": "card_010",
      "properties": {},
      "display": {
        "rank": "8",
        "suit": "红桃"
      }
    }
  ],
  "publicProperties": {
    "本轮应出牌者": "user_001"
  }
}
```

### 等待玩家输入

后端执行到 `出牌组件(type=21)` 时，向当前玩家发送：

```json
{
  "event": "game.pending_action",
  "payload": {
    "sessionId": "game_001",
    "actionId": "action_001",
    "type": "play_cards",
    "playerId": "user_001",
    "timer": 30,
    "deadlineAt": 1710000030000,
    "constraints": {
      "cardRule": "单张",
      "canSkip": true,
      "lastPlayedBy": "user_002",
      "lastPlayedCards": []
    }
  }
}
```

后端执行到 `动作选择组件(type=22)` 时：

```json
{
  "event": "game.pending_action",
  "payload": {
    "sessionId": "game_001",
    "actionId": "action_002",
    "type": "choose_option",
    "playerId": "user_001",
    "timer": 30,
    "deadlineAt": 1710000030000,
    "options": [
      {
        "name": "叫地主",
        "value": 1
      },
      {
        "name": "不叫",
        "value": 0
      }
    ]
  }
}
```

### 提交玩家输入

出牌：

```http
POST /api/games/:sessionId/actions/:actionId/play-cards
```

请求：

```json
{
  "cardIds": ["card_001", "card_002"]
}
```

跳过：

```http
POST /api/games/:sessionId/actions/:actionId/skip
```

动作选择：

```http
POST /api/games/:sessionId/actions/:actionId/choose
```

请求：

```json
{
  "value": 1
}
```

后端收到输入后：

- 校验 `actionId` 是否仍有效。
- 校验提交玩家是否为目标玩家。
- 校验卡牌是否属于该玩家手牌。
- 根据规则的牌型构建、牌型比较和出牌流程判断输入是否有效。
- 更新运行时状态并继续执行流程。
- 广播最新 `game.snapshot` 或增量 `game.updated`。

无效输入响应：

```json
{
  "success": false,
  "message": "Selected cards do not match the required cardset.",
  "data": {
    "reason": "invalid_cardset"
  }
}
```

### 对局 HTTP 辅助接口

用于页面刷新、断线重连后的快照恢复：

```http
GET /api/games/current?roomCode=A12BCD
GET /api/games/:sessionId
```

响应：

```json
{
  "success": true,
  "data": {
    "sessionId": "game_001",
    "roomCode": "A12BCD",
    "status": "playing",
    "currentPlayerId": "user_001",
    "players": [],
    "table": {},
    "handCards": [],
    "pendingAction": null,
    "lastAction": null
  }
}
```

## WebSocket 事件建议

服务端发给客户端：

| 事件 | 说明 |
| --- | --- |
| `room.snapshot` | 订阅后返回房间完整快照 |
| `room.updated` | 玩家加入、离开、准备、房主变化 |
| `game.started` | 房主开始游戏，前端跳转或进入 battle |
| `game.snapshot` | 对局完整快照，适合页面初始化和重连 |
| `game.updated` | 对局状态增量更新 |
| `game.pending_action` | 请求某个玩家出牌或选择动作 |
| `game.action_rejected` | 当前玩家提交的动作不合法 |
| `game.finished` | 结算完成 |
| `error` | 通用错误 |

客户端发给服务端：

| 事件 | 说明 |
| --- | --- |
| `room.subscribe` | 订阅房间频道 |
| `room.unsubscribe` | 取消订阅 |
| `game.subscribe` | 订阅对局频道 |
| `game.heartbeat` | 心跳，辅助判断在线状态 |

玩家操作也可以走 WebSocket，但 alpha 阶段建议玩家输入先走 HTTP，状态广播走 WebSocket，这样更容易调试和写测试。

## 后端规则执行职责

后端是自定义规则状态机，应负责：

- 根据规则 JSON 初始化玩家、牌、牌桌对象。
- 执行 `match_flow`，遇到 `type=21` 或 `type=22` 暂停并等待玩家输入。
- 执行牌型判断、牌型内部比较、牌型间比较。
- 执行 `end_flow` 生成结算结果。
- 维护权威手牌、牌桌、当前玩家、上次有效出牌和动作结果。
- 对每个玩家返回权限过滤后的快照，不能泄露其他玩家手牌。

前端不应负责最终合法性判断。前端可以做体验上的预校验，但后端必须重新校验。

## 前端改造建议

### `src/api/rule.ts`

从 localStorage 替换为真实接口：

```ts
saveRuleDesign(payload: {
  name: string
  playerCount: number
  description: string
  design: ExportedRuleDesign
})
```

建议同时加：

- `getDraft(id)`
- `updateDraft(id, payload)`
- `publishDraft(id)`
- `getRuleOptions()`

### `src/api/room.ts`

短期可保留现有方法名，只把 mockFn 关闭并对齐后端响应：

- `getRuleOptions`
- `createRoom`
- `joinRoom`
- `checkRoomPassword`
- `getCurrentRoom`
- `getRoomByCode`
- `setReady`
- `startGame`
- `leaveRoom`

建议新增 `hasPassword` 字段，前端不要依赖返回的 `password`。

### `BattleView.vue`

需要从静态演示数据改为：

- 根据路由 `roomCode` 获取当前 `gameSession`。
- 连接 WebSocket 订阅房间/对局。
- 用 `GameSnapshot.handCards` 渲染自己的手牌。
- 用 `GameSnapshot.players[*].cardCount` 渲染其他玩家。
- 用 `pendingAction` 控制 PLAY/SKIP/选项按钮是否可用。
- 提交出牌或动作后等待后端广播新状态。

## 推荐实施顺序

1. 后端先实现规则草稿保存、读取、发布和 `/api/room/rules`。
2. 前端把 `ruleApi.saveRuleDesign` 改为提交 `{ metadata, design }`。
3. 后端实现房间 REST 接口，并用真实存储替换前端 localStorage mock。
4. 接入 WebSocket，让准备房不再依赖轮询。
5. 后端实现最小规则状态机，先跑通洗牌、发牌、出牌、结束对局。
6. 改造 `BattleView.vue` 读取真实 `GameSnapshot`。
7. 补充更严格的规则校验和错误提示。

## 最小验收标准

alpha 阶段完成时，至少应能跑通：

1. 用户登录。
2. 在规则构建器保存并发布一个规则。
3. 创建房间时能选到该规则。
4. 其他玩家通过房间号加入。
5. 全员 ready 后房主开始游戏。
6. 后端创建对局并向前端推送初始手牌和当前玩家。
7. 当前玩家出牌或跳过后，其他玩家能实时看到牌桌和手牌数量变化。
8. 对局进入结算后，前端能显示结束状态。

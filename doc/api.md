## 用户系统

### 用户注册

创建新用户账号。

- **URL**: `/api/user/register`
- **Method**: `POST`

- **请求体 (JSON)**:

  | 参数        | 类型   | 说明                                  |
  | :---------- | :----- | :------------------------------------ |
  | `email`     | String | 邮箱地址                              |
  | `user_name` | String | 用户名                                |
  | `password`  | String | 密码 (原始字符串，后台会进行哈希处理) |

- **响应**:
  - `200 OK`: 注册成功。
  - `409 Conflict`：
    - 用户名已存在。
    - 邮箱地址已存在。
  - 其他错误码见 **错误响应** 节。

### 用户登录

验证用户名和用户密码并建立会话。

- **URL**: `/api/user/login`
- **Method**: `POST`

- **请求体 (JSON)**:

  | 参数        | 类型   | 说明   |
  | :---------- | :----- | :----- |
  | `user_name` | String | 用户名 |
  | `password`  | String | 密码   |

- **响应**:
  - `200 OK`: 登录成功，返回 token。
    - **Header**: 在响应头中设置 `Set-Cookie: token=...`，浏览器会自动设置并记录 Cookie，不需要前端操作。
    - **响应**：相应正文也会以 JSON 格式返回

      ```json
      {
        "token": "eyJhbGciOiJIUzI1..."
      }
      ```

      | 字段    | 类型   | 说明 |
      | :------ | :----- | :--- |
      | `token` | String |      |

  - `401 Unauthorized`: 密码错误或用户不存在。
  - 其他错误码见 **错误响应** 节。

### 用户登出

清除用户会话 Cookie。

- **URL**: `/api/user/logout`
- **Method**: `GET`
- **响应**:
  - `200 OK`: 登出成功。响应头包含 `Set-Cookie: token=;` 使浏览器立即删除 Cookie，也不需要前端操作即可登出。
  - 其他错误码见 **错误响应** 节。

### 查找用户

根据用户名获取用户基本信息。

- **URL**: `/api/user/find`
- **Method**: `GET`

- **Query 参数**:

  | 参数        | 类型   | 说明           |
  | :---------- | :----- | :------------- |
  | `user_name` | String | 要查找的用户名 |

- **响应**
  - `200 OK`:
    响应为 JSON 格式。

    | 字段        | 类型   | 说明     |
    | :---------- | :----- | :------- |
    | `email`     | String | 邮箱地址 |
    | `user_name` | String | 用户名   |
    | `user_id`   | String | 用户 ID  |

  - `404 Not Found`：用户不存在。
  - 其他错误码见 **错误响应** 节。

### 获取当前用户信息

验证当前 Token 并返回完整用户对象。

- **URL**: `/api/user/me`
- **Method**: `GET`
- **认证要求**: 必须携带合法的 `token` Cookie。

- **响应**
  - `200 OK`:
    响应为 JSON 格式。

    | 字段        | 类型   | 说明     |
    | :---------- | :----- | :------- |
    | `email`     | String | 邮箱地址 |
    | `user_name` | String | 用户名   |
    | `user_id`   | String | 用户 ID  |

  - `401 Unauthorized`：登录认证不通过。
  - 其他错误码见 **错误响应** 节。

## 房间

### 创建房间

- **URL**: `/api/room/create`
- **Method**: `POST`
- **认证要求**: 必须携带合法的 `token` Cookie。

- **请求体（JSON）**

  | 参数              | 类型                      | 说明                                                     |
  | :---------------- | :------------------------ | :------------------------------------------------------- |
  | `room_password`   | String，可选，默认值 `""` | 房间密码，如果房间不需要密码，不必发此参数，将密码留空。 |
  | `player_capacity` | int                       | 房间用户容量                                             |
  | `rule`            | RuleDefinition，可选      | 房间绑定的规则定义，可不传。                             |

- **响应**
  - `200 OK`:
    响应为 JSON 格式。

    响应字段和说明与 **查找房间** 一致。

  - 其他错误码见 **错误响应** 节。

### 查找房间

- **URL**：`/api/room/find`
- **Method**：`GET`

- **Query 参数**:

  | 参数           | 类型                      | 说明                                                 |
  | :------------- | :------------------------ | :--------------------------------------------------- |
  | `room_id`      | String                    | 根据房间 ID 查找房间信息                             |
  | `sharing_code` | int, 取值区间 [0,999999]  | 根据分享码查找房间                                   |
  | `password`     | String，可选，默认值 `""` | 如果当前房间设置了密码，只有密码正确才能查看房间信息 |

  其中 `room_id` 与 `sharing_code` 这两个参数只能二选一。

- **响应**
  - `200 OK`:
    响应为 JSON 格式。

    | 字段           | 类型                     | 说明                                      |
    | :------------- | :----------------------- | :---------------------------------------- |
    | `room_id`      | String                   | 房间 ID                                   |
    | `sharing_code` | int, 取值区间 [0,999999] | 分享码，作为用户查找房间的凭证            |
    | `owner`        | String (user_id)         | 当前房间的所有者 ID，通常是房间的创建者。 |
    | `players`      | List[String \| null]     | 当前房间的玩家以及空座位。                |

  - `401 Unauthorized`：密码不正确。
  - `404 Not Found`：没有查找到结果。

### 删除房间

所有者可以决定是否直接删除房间。

- **URL**：`/api/room/delete`
- **Method**：`POST`

- **Query 参数**:

  | 参数      | 类型   | 说明            |
  | :-------- | :----- | :-------------- |
  | `room_id` | String | 待删除房间的 ID |

- **响应**
  - `200 OK`：删除成功。

### 获取房间规则

查询房间当前绑定的规则定义。

- **URL**：`/api/room/rule/get`
- **Method**：`GET`

- **Query 参数**:

  | 参数       | 类型                      | 说明     |
  | :--------- | :------------------------ | :------- |
  | `room_id`  | String                    | 房间 ID  |
  | `password` | String，可选，默认值 `""` | 房间密码 |

- **响应**
  - `200 OK`:

    | 字段      | 类型                   | 说明                        |
    | :-------- | :--------------------- | :-------------------------- |
    | `room_id` | String                 | 房间 ID                     |
    | `rule`    | RuleDefinition \| null | 当前规则，未绑定则为 `null` |

  - `401 Unauthorized`：房间密码错误。
  - `404 Not Found`：房间不存在。

### 加入房间并建立 WebSocket

通过 WebSocket 加入房间并接收房间状态广播。

- **URL**: `/api/room/enter`
- **Method**: `GET` 或 WebSocket Upgrade
- **认证要求**: 必须携带合法的 `token` Cookie。

- **Query 参数**:

  | 参数         | 类型                      | 说明             |
  | :----------- | :------------------------ | :--------------- |
  | `room_id`    | String                    | 房间 ID          |
  | `seat_index` | usize                     | 要占用的座位序号 |
  | `password`   | String，可选，默认值 `""` | 房间密码         |

- **服务端下行消息**

  所有服务端下行消息均为 JSON，结构为 `RoomEvent`。

  | 类型           | 说明                                 |
  | :------------- | :----------------------------------- |
  | `Snapshot`     | 进入房间后发送一次，表示当前房间快照 |
  | `PlayerJoined` | 有玩家进入房间                       |
  | `PlayerLeft`   | 有玩家离开房间                       |
  | `RuntimeEvent` | 规则运行时事件                       |
  | `StateChanged` | 房间状态变化，例如心跳触发的状态同步 |
  | `Error`        | 协议解析或运行错误                   |

- **客户端上行消息**

  客户端发送的消息格式为 `ClientRoomMessage`。

  | 类型        | 说明                       |
  | :---------- | :------------------------- |
  | `Heartbeat` | 心跳消息，用于触发状态同步 |
  | `Leave`     | 主动离开房间               |
  | `Emit`      | 向服务端发送一个业务事件   |
  | `Command`   | 向服务端发送一个操作指令   |

  `Emit` 和 `Command` 的 JSON 结构包含 `name` 和 `payload` 两个字段，其中 `payload` 的值类型为 `RuleValue`。

# 错误响应

### RuleDefinition

房间的规则定义（`RuleDefinition`）是一个完全由 JSON 表达的对象，前端只需要按下面的结构组装请求体即可，不需要查看后端源码。

#### 顶层结构

```json
{
  "name": "standard-game",
  "player_count": 4,
  "classes": {},
  "cardsets": {},
  "match_flow": {},
  "end_flow": {}
}
```

| 字段           | 类型   | 必填 | 说明                        |
| :------------- | :----- | :--- | :-------------------------- |
| `name`         | String | 是   | 规则名称                    |
| `player_count` | int    | 是   | 玩家人数                    |
| `classes`      | object | 否   | 类定义表，键是类名          |
| `cardsets`     | object | 否   | 牌组/集合定义表，键是集合名 |
| `match_flow`   | object | 否   | 比赛流程图，键是节点 ID     |
| `end_flow`     | object | 否   | 结束流程图，键是节点 ID     |

#### `ClassDefinition`

```json
{
  "default_properties": {},
  "user_properties": {},
  "methods": {}
}
```

| 字段                 | 类型   | 必填 | 说明                         |
| :------------------- | :----- | :--- | :--------------------------- |
| `default_properties` | object | 否   | 默认属性表，键是属性名       |
| `user_properties`    | object | 否   | 玩家可配置属性表，键是属性名 |
| `methods`            | object | 否   | 方法定义表，键是方法名       |

#### `CardSetDefinition`

```json
{
  "name": "main-deck",
  "properties": {},
  "build_flow": {},
  "compare_flow": {},
  "successors": []
}
```

| 字段           | 类型     | 必填 | 说明                   |
| :------------- | :------- | :--- | :--------------------- |
| `name`         | String   | 是   | 集合名称               |
| `properties`   | object   | 否   | 卡牌属性表，键是属性名 |
| `build_flow`   | object   | 否   | 构建集合的流程图       |
| `compare_flow` | object   | 否   | 比较集合元素的流程图   |
| `successors`   | string[] | 否   | 后继集合名称列表       |

#### `PropertyDefinition`

```json
{
  "type": "Integer",
  "default": 0,
  "config": []
}
```

| 字段      | 类型           | 必填 | 说明                         |
| :-------- | :------------- | :--- | :--------------------------- |
| `type`    | `ValueType`    | 是   | 属性类型                     |
| `default` | `RuleValue`    | 是   | 默认值                       |
| `config`  | `EnumOption[]` | 否   | 枚举选项，仅 `Enum` 类型常用 |

`EnumOption` 结构：

```json
{ "display": "A", "value": 1 }
```

#### `ValueType`

`type` 字段采用以下枚举之一：

- `Integer`
- `Collection`，表示集合类型，内部再包一层 `ValueType`
- `Enum`，表示枚举引用，结构如下：

```json
{ "Enum": { "class_name": "Card", "property_name": "suit" } }
```

#### `MethodDefinition` 与 `MethodParameter`

```json
{
  "parameters": {},
  "returns": null,
  "flow": {}
}
```

| 字段         | 类型                | 必填 | 说明                            |
| :----------- | :------------------ | :--- | :------------------------------ |
| `parameters` | object              | 否   | 参数表，键是参数名              |
| `returns`    | `ValueType \| null` | 否   | 返回值类型，`null` 表示无返回值 |
| `flow`       | object              | 否   | 方法执行流程图                  |

`MethodParameter` 只有一个字段：

```json
{ "type": "Integer" }
```

#### `FlowGraph` 与 `FlowNode`

`FlowGraph` 是一个对象，键是节点 ID，值是 `FlowNode`。

`FlowNode` 统一使用 `kind/data` 结构：

```json
{
  "kind": "Start",
  "data": {
    "kind": "Generic",
    "next": "2"
  }
}
```

常用 `FlowNodeContent` 类型：

| kind               | 说明                                                                                     |
| :----------------- | :--------------------------------------------------------------------------------------- |
| `Start`            | 流程入口，`data.kind` 可选 `Generic`、`Game`、`Method`、`Match`、`Compare`、`Settlement` |
| `Nop`              | 空操作                                                                                   |
| `Assignment`       | 赋值，`target` + `value`                                                                 |
| `Branch`           | 条件分支，`condition` + `on_true` / `on_false`                                           |
| `SortCollection`   | 对集合排序                                                                               |
| `SelectCollection` | 从集合选择元素                                                                           |
| `Emit`             | 产生一个运行时事件                                                                       |
| `Call`             | 调用对象方法                                                                             |
| `MatchCards`       | 牌面匹配                                                                                 |
| `CompareReturn`    | 比较后直接返回 `A/B/Tie`                                                                 |
| `Return`           | 返回结果                                                                                 |

#### `RuleValue`

`payload`、变量和属性值统一使用 `RuleValue`，支持以下类型：

- `Integer`
- `Enum`
- `Boolean`
- `Text`
- `List`
- `Object`
- `Null`

示例：

```json
{
  "Integer": 1
}
```

```json
{
  "Text": "hello"
}
```

```json
{
  "Object": {
    "score": { "Integer": 10 },
    "name": { "Text": "alice" }
  }
}
```

#### 绑定策略

房间应在创建时通过 `/api/room/create` 的 `rule` 字段一次性绑定规则。绑定后规则不可更改，后端不再提供修改接口。

#### 推荐前端做法

如果前端要动态编辑规则，建议直接按照上面的 JSON 结构生成表单，然后把整个 `RuleDefinition` 对象原样提交给 `/api/room/create`。

- `404 Not Found`
  - 没有查找到相应结果。
- `400 Bad Request`
  - 请求参数错误。
- `500 Internal Server Error`：
  - 数据库写入/读取失败
  - 加密错误（在验证用户登录时）

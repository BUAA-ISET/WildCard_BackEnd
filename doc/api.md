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

  | 参数 | 类型 | 说明 |
  | :--- | :--- | :--- |
  | `room_id` | String | 房间 ID |
  | `password` | String，可选，默认值 `""` | 房间密码 |

- **响应**
  - `200 OK`:

    | 字段 | 类型 | 说明 |
    | :--- | :--- | :--- |
    | `room_id` | String | 房间 ID |
    | `rule` | RuleDefinition \| null | 当前规则，未绑定则为 `null` |

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

房间的规则定义（`RuleDefinition`）为房间绑定具体玩法规则的 JSON 对象。主要字段：

- `name` (String): 规则名称。
- `player_count` (int): 期望的玩家数量（用于校验和脚本中的玩家索引）。
- `classes` (object): 类型/类定义的映射（可选）。
  - 每个 `ClassDefinition` 包含 `default_properties`, `user_properties`, `methods` 等。
- `cardsets` (object): 牌组/集合定义的映射（可选）。
  - 每个 `CardSetDefinition` 包含 `name`, `properties`, `build_flow`, `compare_flow`, `successors` 等。
- `match_flow` (FlowGraph): 比赛进行流程的有向图（节点 ID -> 节点）。
- `end_flow` (FlowGraph): 结束/结算流程的有向图（节点 ID -> 节点）。

FlowGraph 与 FlowNode 的结构为可序列化的节点映射，节点采用 `{ "kind": ..., "data": ... }` 的 tag/content 格式，可包含下列节点类型示例：

- `Start`：流程入口。示例： `{ "kind": "Start", "data": { "kind": "Generic", "next": "1" } }`。
- `Assignment`：赋值。示例： `{ "kind": "Assignment", "data": { "target": "var", "value": { "kind": "Constant", "data": { "Integer": 1 } }, "next": "2" } }`。
- `Emit`：向运行时发出事件（会成为 `RuntimeEvent`）。
- `SortCollection` / `SelectCollection`：对集合进行排序/取样。
- `Call`：调用对象方法（可触发本地/native 方法）。
- `Return`：从流程返回值。

完整的 `RuleDefinition` 结构定义请参见后端 `src/domain/rule.rs` 中的 `RuleDefinition`, `ClassDefinition`, `CardSetDefinition`, `FlowGraph` 与 `FlowNodeContent`，前端在构造规则 JSON 时应遵循这些字段与枚举 tag。

绑定策略：房间应在创建时（`/api/room/create` 的 `rule` 字段）指定规则。规则一旦绑定后不可更改。

- `404 Not Found`
  - 没有查找到相应结果。
- `400 Bad Request`
  - 请求参数错误。
- `500 Internal Server Error`：
  - 数据库写入/读取失败
  - 加密错误（在验证用户登录时）

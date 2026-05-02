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

# 错误响应

- `404 Not Found`
- `400 Bad Request`
- `400 Bad Request`
- `500 Internal Server Error`：
  - 数据库写入/读取失败
  - 加密错误（在验证用户登录时）

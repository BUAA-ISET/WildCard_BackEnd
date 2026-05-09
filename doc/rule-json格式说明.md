# Rule JSON 格式说明

本文说明后端当前支持的规则 JSON 结构。对应 Rust 类型定义位于 `src/domain/rule_engine.rs`。

## 1. 顶层结构

规则文件顶层是一个 JSON object，包含 5 个主要字段：

```json
{
  "classes": {},
  "cardsets": {},
  "cardset_comparisons": {},
  "match_flow": {},
  "end_flow": {}
}
```

字段含义：

- `classes`：系统运行时对象类型定义，目前固定需要包含 `player`、`card`、`table`
- `cardsets`：牌型定义
- `cardset_comparisons`：跨牌型比较规则
- `match_flow`：对局主流程
- `end_flow`：结算流程

后端校验要求：

- `classes` 必须包含 `player`、`card`、`table`
- `cardsets` 至少有 1 个
- 所有名字不能重复，且不能为空

## 2. classes

### 2.1 结构

```json
{
  "player": {
    "default_properties": {},
    "user_properties": {},
    "methods": {}
  }
}
```

每个 class 结构如下：

```json
{
  "default_properties": {
    "属性名": {
      "type": "int | enum",
      "default": 0,
      "config": []
    }
  },
  "user_properties": {},
  "methods": {}
}
```

说明：

- `default_properties`：内置属性
- `user_properties`：规则作者自定义属性
- `methods`：对象方法

当前后端支持的属性类型只有：

- `int`
- `enum`

其中：

- `int` 只需要 `type` 和 `default`
- `enum` 除了 `type` 和 `default` 外，通常还需要 `config`

### 2.2 enum 属性格式

```json
{
  "point": {
    "type": "enum",
    "default": 1,
    "config": [
      { "id": "enum-point-1", "display": "A", "value": 1 },
      { "id": "enum-point-2", "display": "2", "value": 2 }
    ]
  }
}
```

后端实际使用的是：

- `default`
- `config[*].value`

`display` 主要给前端显示用。

## 3. methods

对象方法结构：

```json
{
  "方法名": {
    "parameters": {
      "paramA": { "type": "int" }
    },
    "returns": "int",
    "flow": {}
  }
}
```

说明：

- `parameters`：参数定义
- `returns`：返回类型，可以是 `int`、`enum` 或 `null`
- `flow`：方法流程图

方法流程中会用到：

- `25`：方法开始
- `26`：返回

## 4. cardsets

牌型定义格式：

```json
{
  "1": {
    "name": "Single",
    "properties": {},
    "build_flow": {},
    "compare_flow": {},
    "successors": []
  }
}
```

字段含义：

- `name`：牌型名称
- `properties`：牌型识别结果上可附带的整型属性
- `build_flow`：判断一组牌是否匹配该牌型
- `compare_flow`：同牌型比较逻辑
- `successors`：当当前牌型可压过哪些其他牌型时，填对方牌型的 `id`

### 4.1 build_flow

用途：识别当前选牌是否属于该牌型。

常见结构：

- `27` 开始
- 若干取值/比较节点
- `16` 条件分支
- `28` 返回匹配结果

`28` 节点格式：

```json
{
  "type": 28,
  "content": {
    "result": 1,
    "properties": {
      "weight": 10
    }
  }
}
```

说明：

- `result = 1` 表示匹配成功
- `result = 0` 表示匹配失败
- `properties` 中目前只会提取整型字段

### 4.2 compare_flow

用途：同牌型比较，判断 A 和 B 谁赢。

常见结构：

- `29` 开始
- 若干计算节点
- `16` 条件分支
- `30` 返回比较结果

`30` 节点格式：

```json
{
  "type": 30,
  "content": {
    "result": 0
  }
}
```

说明：

- `result = 0` 表示 A 胜
- `result = 1` 表示 B 胜

这里的 A / B 在运行时分别对应：

- A：当前出牌
- B：上一手成功出牌

## 5. cardset_comparisons

用于跨牌型比较。

格式：

```json
{
  "1": {
    "cardsetA": "Single",
    "cardsetB": "Bomb",
    "compare_flow": {}
  }
}
```

匹配方式基于牌型 `name`，不是 `id`。

后端逻辑：

- 如果当前出牌与上一手牌型不同
- 优先查 `cardset_comparisons`
- 如果没查到，再看 `successors`

## 6. FlowGraph 格式

所有流程图字段都使用同一种结构：`FlowGraph`

```json
{
  "1": {
    "type": 17,
    "content": {
      "next": "2"
    }
  },
  "2": {
    "type": 21,
    "content": {
      "timer": 30,
      "next": "3"
    }
  }
}
```

每个节点的基础结构：

```json
{
  "type": 17,
  "content": {},
  "count": 1,
  "next": "2"
}
```

字段说明：

- `type`：组件类型编号
- `content`：组件配置
- `count`：主要给发牌组件用
- `next`：直接后继节点

后端还会从 `content` 里读取：

- `next`
- `next_true`
- `next_false`

因此很多旧规则会把跳转写在 `content` 里，这也是兼容的。

### 6.1 节点编号约定

- 流程入口必须存在编号 `"1"`
- 所有跳转目标必须存在
- 分支缺失 `next_true` 或 `next_false` 会报错

## 7. 当前后端支持的组件类型

当前支持：

- `1` 方法调用
- `2` 排序
- `4` 赋值
- `5` 取集合第 N 个元素
- `6` 属性访问
- `7` 集合长度
- `8` / `9` 常量整数
- `10` 四则运算
- `11` 集合逻辑
- `12` 二元逻辑
- `13` 非
- `14` 比较
- `15` 牌型判断
- `16` 条件分支
- `17` 对局开始
- `18` 进入结算
- `19` 洗牌
- `20` 发牌
- `21` 等待玩家出牌
- `22` 等待玩家做选择
- `23` 结算开始
- `24` 写入单个玩家结算结果
- `25` 方法开始
- `26` 返回
- `27` 牌型构建开始
- `28` 牌型构建返回
- `29` 牌型比较开始
- `30` 牌型比较返回

## 8. 常用引用值

后端在求值时支持一些保留引用。

### 8.1 对象引用

- `table_0`：牌桌对象
- `player_0`、`player_1`...：按运行时座位顺序访问玩家
- `玩家`：当前玩家

注意：

- 当前玩家不是按用户真实 ID 决定，而是按运行时座位顺序决定

### 8.2 牌组/牌型引用

- `cards`
- `card_set`
- `selectedCards`
- `selected_cards`

以上都表示“当前传入牌组”。

牌型比较流程中：

- `A` / `a` / `cardsetA` / `currentRound`：当前出牌
- `B` / `b` / `cardsetB` / `previousRound`：上一手成功出牌

### 8.3 表属性

后端会额外维护一些运行时表属性，例如：

- `player_index`
- `settlement_index`
- `cur_max`

规则里也可以自己定义额外的 `table.user_properties`。

## 9. 属性访问规则

`type = 6` 的 `content` 结构：

```json
{
  "operator": 0,
  "ident": "table_0",
  "property": "player_index"
}
```

`operator` 含义：

- `0`：对象属性访问
- `1`：组件结果属性访问
- `2`：集合映射属性访问

示例：

```json
{
  "type": 6,
  "content": {
    "operator": 1,
    "ident": "2",
    "property": "point"
  }
}
```

表示读取节点 `2` 结果的 `point` 属性。

## 10. 比较运算符

`type = 14` 时，`operator` 当前含义为：

- `0`：等于
- `1`：大于
- `2`：小于
- `3`：大于等于
- `4`：小于等于

## 11. 最小可运行样例

当前仓库内有两个样例文件：

- `test.json`：历史规则样例
- `test2.json`：当前极简可手测样例

建议以后排查规则问题时优先从 `test2.json` 开始，因为它结构最小、链路更短。

## 12. 编写建议

- 优先使用 ASCII 属性名，如 `point`、`suit`、`winner_index`
- 不要依赖前端导出时的乱码中文键名
- 分支节点 `condition` 不要留空
- 组件结果取属性时，确认 `type = 6` 的 `operator` 是否应为 `1`
- 牌型比较返回值里，`0` 是 A 赢，不是“false”
- 如果规则需要版本化，建议直接把 JSON 放进仓库，而不是只放在工作区根目录

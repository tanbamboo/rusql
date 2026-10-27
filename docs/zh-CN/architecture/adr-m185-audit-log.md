# ADR：M185 可选 JSON 审计日志

**状态**：已接受（M185 / [#337](https://github.com/tanbamboo/rusql/issues/337)）  
**日期**：2026-10-27

## 背景

运维有时需要记录谁连上了服务器、执行了哪些语句。MySQL 企业版用闭源 `audit_log` 插件提供这件事（XML 或它自己的 JSON 结构、过滤器和轮转）。rusql 不加载该插件，也不宣称兼容。

本切片是 rusql 进程上的一个可选文件。不传入路径时关闭。不修改 `CONSTITUTION.md`。`performance_schema` 仍属于 M189。企业插件仍属于 M188。

## 决策

1. **标志 `--audit-log=PATH`。** 不传则不创建审计文件。传入后，进程在接受连接之前创建或追加该文件。缺少的父目录会被创建。空路径或无法打开的路径会让进程以 i18n 错误退出。打开成功时，进程向 stderr 写一行：此日志不是 Oracle MySQL 企业版审计插件。

2. **JSON 行，四个字段。** 每个事件是一个 JSON 对象、单独一行：`timestamp`（UTC `YYYY-MM-DDTHH:MM:SS.mmmZ`）、`user`、`command`、`status`。没有 XML 模式，没有企业版过滤器，没有轮转。

3. **记录什么。**
   - 握手成功之后：`command` 为 `Connect`，`status` 为 0。
   - 每一条完成的 `COM_QUERY`：一行。`command` 是压成一行的语句文本。响应里没有 ERR 包时 `status` 为 0，否则是第一个 ERR 包里的 MySQL errno（例如 1146）。
   - 该会话结束时：`command` 为 `Quit`，`status` 为 0。成功的 `COM_CHANGE_USER` 会更新这一行的用户。
   - 失败的握手不写行。认证包不是审计字段。

4. **隐私。** 不写结果集的行。`COM_STMT_EXECUTE` 和其他命令不是查询行。`COM_CHANGE_USER` 的包里带有密码，因此也不是查询行。在 `COM_QUERY` 文本中，`IDENTIFIED BY`、`IDENTIFIED BY PASSWORD`、`IDENTIFIED WITH … BY|AS`、`SET PASSWORD … =` 和 `PASSWORD(` 后面的字面量替换为 `'<redacted>'`。这个记号在所有语言环境里都是文件格式的一部分。

5. **写失败不改变客户端结果。** 磁盘满时服务器打一条警告，语句仍返回它原来的 errno。客户端看不到新的审计 errno。

## 后果

- 文件会一直变大。本切片不做轮转。
- `mysql-diff` 仍然只比较 SQL，不读这个文件。检查命令是 `cargo test -p rusql-server audit_log`。
- SQL 文本里的密码只以脱敏形式保存。只出现在结果行里的值不会保存。`INSERT` 里的字面量属于语句文本，因此会保存一次。

## 不在范围内

- Oracle `audit_log` 插件、XML、查询过滤器和轮转
- 记录结果行、握手密码或 `COM_CHANGE_USER` 载荷
- `performance_schema`（M189）
- 把 `COM_STMT_EXECUTE` 当成单独的审计事件

## 如何验证

```bash
cargo test -p rusql-server audit_log
```

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data --audit-log ./audit.jsonl
```

连接、执行 `SELECT 1`、断开。文件里有一行 `Connect`、一行 `"status":0` 的 `SELECT 1`，以及一行 `Quit`。不带 `--audit-log` 启动不会创建该文件。

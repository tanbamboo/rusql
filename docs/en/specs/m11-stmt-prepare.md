# M11: COM_STMT_PREPARE / EXECUTE / CLOSE

## Goal

MySQL binary prepared-statement commands for drivers and clients.

## Acceptance criteria

- [x] `COM_STMT_PREPARE` returns statement id and metadata
- [x] `COM_STMT_EXECUTE` with VARCHAR / integer params via `?`
- [x] `COM_STMT_CLOSE` releases handle
- [x] Wire integration tests

## Boundaries

- ~~No binary resultset~~ → **M25** adds binary `COM_STMT_EXECUTE` resultset
- No `COM_STMT_FETCH`, no long-data (0x18)

## Decisions

| Topic | Choice |
|-------|--------|
| Placeholder binding | Substitute `?` with literals before `sqlparser` |
| Param types | VARCHAR and LONGLONG on execute |

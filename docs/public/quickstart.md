# Quickstart

## 1. Install

```bash
curl -fsSL https://agentralabs.tech/install/codebase | bash
```

Profile-specific commands are listed in [Installation](installation.md).

## 2. Compile a repository

```bash
acb compile ./my-project -o project.acb
acb info project.acb
```

## 3. Run core queries

```bash
acb query project.acb symbol --name "main"
acb query project.acb impact --unit-id 1
acb query project.acb deps --unit-id 1 --depth 3
```

## 4. Start MCP server

```bash
$HOME/.local/bin/acb-mcp
```

Use `Ctrl+C` to stop after startup verification.

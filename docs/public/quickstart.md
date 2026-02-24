---
status: stable
---

# Quickstart

## 1. Install

```bash
curl -fsSL https://agentralabs.tech/install/codebase | bash
```

Profile-specific commands are listed in [Installation](installation.md).

## 2. Compile a repository

```bash
acb compile ./my-project -o project.acb --coverage-report coverage.json
acb info project.acb
```

## 3. Run core queries

```bash
acb query project.acb symbol --name "main"
acb query project.acb impact --unit-id 1
acb query project.acb deps --unit-id 1 --depth 3
acb query project.acb test-gap
acb query project.acb hotspots
acb query project.acb dead-code
```

## 4. Run health and gate checks

```bash
acb health project.acb
acb gate project.acb --unit-id 1 --max-risk 0.60 --require-tests
acb budget project.acb --horizon-years 20 --max-bytes 2147483648
```

## 5. Start MCP server

```bash
acb-mcp serve
```

Use `Ctrl+C` to stop after startup verification.

## Validate capabilities

```bash
./scripts/test-primary-problems.sh
```

See [Experience With vs Without](experience-with-vs-without.md) for the full capability map.

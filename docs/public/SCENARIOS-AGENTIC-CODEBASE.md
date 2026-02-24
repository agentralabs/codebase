# Capability Scenarios: AgenticCodebase

*What happens when an AI agent understands your code the way an architect understands a building — structurally, historically, and predictively?*

---

## IMPACT Edges — Knowing What Breaks Before You Break It

You're about to rename the `calculateTotal` function in your e-commerce payment module. It's a small change — just improving a name. You do a quick text search, find 8 references, update them, and push. The tests pass locally. But in production, a webhook handler in a completely different service imports `calculateTotal` dynamically via a string-based loader, and your rename just broke payment processing for 40,000 users.

Without impact analysis, renaming a function is a best-effort text search. You find the references you can see, miss the ones you can't, and hope for the best. The dynamic import, the reflection-based caller, the test helper that constructs function names from strings — these are invisible to grep. The blast radius of your change is unknown until something breaks in production.

With IMPACT edges, the agent runs `impact_analysis` on `calculateTotal` with `max_depth: 5` and returns the full dependency tree in 1.46 microseconds. Direct dependents: 8 callers across 3 files (the ones you found). Transitive dependents: 23 additional code units across 7 files (the ones you missed). The webhook handler in `services/webhook/processor.ts` is flagged as a high-risk dependent — it has a stability score of 0.38, complexity of 24, and 7 recent changes. The agent also identifies 3 test files that cover `calculateTotal` directly and 2 integration tests that cover it transitively. Risk summary: 4 high-risk, 8 medium-risk, 19 low-risk dependents.

> Impact analysis for `calculateTotal`: 31 total dependents across 10 files. 4 high-risk paths: `webhook/processor.ts` (dynamic import, stability 0.38), `reports/monthly.ts` (8 callers deep), `api/v2/checkout.ts` (public endpoint), and `batch/reconcile.ts` (nightly job). 3 direct test files, 2 integration suites. Recommend updating the webhook handler's dynamic loader and adding a test for the batch reconciler before proceeding.

**In plain terms:** IMPACT edges are X-ray vision for code changes. Before you touch anything, the agent shows you every piece of code that depends on it — including the hidden, indirect, and dynamically-loaded dependencies that text search misses. You see the blast radius before the explosion.

---

## CALLED_BY / CALLS Edges — Dependency Traversal

You've inherited a legacy codebase with 200,000 lines of Python. You need to understand what the `process_order` function does, but it calls 12 other functions, some of which call 8 more, and those call 5 more. The call tree is 6 levels deep. Reading the code linearly would take hours.

Without call graph traversal, you open `process_order`, see it calls `validate_inventory`, `calculate_shipping`, `apply_discount`, and 9 other functions. You open `validate_inventory`, which calls `check_warehouse_stock` and `reserve_items`. You open `check_warehouse_stock`, which calls `query_inventory_db` and `parse_stock_response`. You're 4 tabs deep, mentally tracking a call tree in your head, losing context faster than you gain it.

With CALLS and CALLED_BY edges, the agent traverses the call graph bidirectionally at 1.27 microseconds for depth 3. Forward traversal from `process_order` reveals the complete call tree: 47 functions called directly or transitively, organized by depth. The agent identifies the critical path: `process_order` → `validate_inventory` → `check_warehouse_stock` → `query_inventory_db` (this is the only path that touches the database, so it's the bottleneck). Reverse traversal reveals who calls `process_order`: the REST endpoint handler, the batch processing job, and a GraphQL resolver — 3 distinct entry points, each with different error handling requirements.

> Call graph for `process_order` (depth 3): 47 callees across 6 levels. Critical database path: `process_order` → `validate_inventory` → `check_warehouse_stock` → `query_inventory_db`. 3 callers: REST handler (`api/orders.py:34`), batch job (`jobs/nightly.py:112`), GraphQL resolver (`graphql/mutations.py:67`). The batch job doesn't implement retry logic — potential reliability gap.

**In plain terms:** Call graph traversal lets the agent read an entire function's extended family tree in microseconds. Instead of you manually tracing calls through files, the agent shows you who calls what, how deep it goes, and where the critical paths are — like a subway map for your code.

---

## TESTS Edges — Coverage Mapping

You've just refactored the `UserAuthentication` class to extract a `TokenValidator` helper. The refactoring is clean — all existing tests pass. But you're not sure which tests actually exercise the extracted logic versus testing the unchanged parts of `UserAuthentication`.

Without test coverage mapping, you run the full test suite and see "47 tests pass." You feel confident. But 43 of those tests exercise login flow, session management, and password reset — parts of `UserAuthentication` you didn't touch. Only 4 tests actually exercise token validation logic, and 2 of those test edge cases that your refactoring changed the behavior of. The 2 affected tests pass only because they use mocked tokens that bypass the new validation path.

With TESTS edges, the agent maps which tests cover which code units. It identifies 47 tests linked to `UserAuthentication` and 4 tests specifically linked to the token validation logic now in `TokenValidator`. It cross-references the change set (the extracted code) against test coverage: 4 tests cover the extracted code, but 2 of them use mocks that skip the validation path entirely. Net real coverage of the refactored code: 2 tests. The agent flags this immediately.

> Test coverage for `TokenValidator` (extracted from `UserAuthentication`): 4 tests reference this code. However, 2 tests (`test_auth_with_mock_token`, `test_expired_mock_token`) use mocked validators that bypass your extracted logic. Real coverage: 2 tests. Recommend adding tests for: invalid signature handling, token rotation edge case, and the new validation error path.

**In plain terms:** TESTS edges connect code to its safety net. Instead of counting green checkmarks and hoping they cover your changes, the agent tells you exactly which tests exercise the code you actually modified — and which tests are lying to you with mocks.

---

## CONTAINS Edges — Structural Hierarchy

You're new to a monorepo with 12 packages, 340 modules, and 2,800 exported symbols. You need to find where user profile logic lives. The directory structure hints (`packages/user/`), but the actual containment hierarchy is more complex — some profile logic lives in `packages/shared/models/`, some in `packages/api/handlers/`, and some in `packages/legacy/compat/`.

Without containment edges, you grep for "profile" and get 287 hits across 43 files. You don't know which are primary implementations versus references. You don't know the module hierarchy — which package contains which module, which module contains which class, which class contains which method. The codebase is a flat list of search results with no structure.

With CONTAINS edges, the agent traverses the structural hierarchy: the `packages/user/` package contains 14 modules, which contain 3 classes and 28 functions related to user profiles. But it also reveals that `packages/shared/models/UserProfile` contains the core data model, `packages/api/handlers/profile_handler` contains the REST endpoints, and `packages/legacy/compat/v1_profile` contains backward-compatibility shims. The containment tree shows you the full structural picture — not just where "profile" appears as a string, but how profile logic is organized across the architecture.

> User profile logic spans 4 packages: `user/` (primary, 14 modules), `shared/models/` (data model, 1 class), `api/handlers/` (REST endpoints, 3 functions), `legacy/compat/` (v1 compatibility, 2 functions). Core class: `shared/models/UserProfile` (contained by `shared/` package). Entry points: `api/handlers/profile_handler.get_profile` and `api/handlers/profile_handler.update_profile`.

**In plain terms:** CONTAINS edges are an organizational chart for your code. Instead of searching for text strings, the agent navigates the containment hierarchy — package contains module, module contains class, class contains method — showing you where things *live*, not just where they're mentioned.

---

## COUPLES_WITH Edges — Shadow Coupling From Git History

Your codebase has no explicit dependency between `pricing/calculator.py` and `notifications/email_templates.py`. They're in different packages. They share no imports. Code review wouldn't connect them. But every time someone changes the pricing calculator, someone changes the email templates within 48 hours — because pricing changes require updating the "your price has changed" notification.

Without temporal coupling detection, this hidden dependency is invisible. A developer changes the pricing formula, the PR passes review, tests pass, and it merges. Two days later, another developer realizes the email templates show outdated prices and submits a panicked follow-up PR. This pattern has repeated 7 times in the last year. Nobody connects the dots because the code-level analysis shows zero relationship.

With COUPLES_WITH edges derived from git history, the agent analyzes commit co-occurrence across the repository. It finds that `pricing/calculator.py` and `notifications/email_templates.py` change together in 89% of commits that touch either file — a coupling strength of 0.89. The coupling type is `Hidden` (no explicit code relationship) with a `Temporal` subtype. The agent flags this during impact analysis when someone modifies the pricing calculator.

> Hidden coupling detected: `pricing/calculator.py` and `notifications/email_templates.py` co-change 89% of the time (12 of 13 commits). No code-level dependency exists — this is a business logic coupling. The email templates reference pricing values that must be updated when pricing formulas change. Recommend: either extract shared pricing constants, or add a CI check that flags pricing changes without corresponding template updates.

**In plain terms:** COUPLES_WITH edges reveal the marriages your codebase never told you about. Two files that always change together are coupled, even if they share zero imports. The agent spots these shadow dependencies by reading git history the way a detective reads phone records — not what people say, but what they actually do together.

---

## PROPHECY — Predicting Failures From Temporal Patterns

Your codebase has a file that's been modified 23 times in the last 30 days. Of those 23 changes, 9 were bugfixes. Its complexity has grown from 150 to 340 lines. Three different developers have touched it. Nobody's noticed because each individual change was small and reasonable.

Without predictive analysis, this file is a ticking time bomb that nobody sees. It's not broken right now. The tests pass right now. It won't show up in any static analysis tool. But the trend lines — change velocity, bugfix ratio, complexity growth, author churn — all point toward imminent failure. You'll discover this when a critical bug escapes to production, and the postmortem reveals that the file was already fragile.

With PROPHECY, the agent analyzes temporal patterns across the codebase and generates predictions weighted by four factors: velocity (30%), bugfix trend (30%), complexity growth (15%), and coupling risk (25%). The file scores a risk of 0.91 — one of the highest in the repository. The prediction: `BugRisk` with reason "23 changes in 30 days, 39% bugfix ratio, complexity doubled, 3 authors — this file is in crisis." The agent also generates an `EcosystemAlert` of type `Hotspot` with severity 0.88.

> PROPHECY alert: `services/payment/refund_handler.py` — risk score 0.91 (BugRisk). 23 changes in 30 days (velocity: top 1%), 39% were bugfixes, complexity doubled (150→340 lines), 3 authors with conflicting patterns. Prediction: high probability of production bug within 2 weeks. Recommend: freeze non-essential changes, add integration tests (current coverage: 2 tests), consider decomposing the handler into smaller units.

**In plain terms:** PROPHECY is a weather forecast for code health. It looks at the patterns — how fast a file is changing, how many of those changes are bugfixes, how many people are touching it — and tells you which files are likely to break before they actually do. Prevention, not reaction.

---

## Stability Scoring — Quantifying Volatility

You're deciding which module to build a new feature on top of. You have two candidates: `services/auth/` (stable, mature) and `services/payments/` (actively evolving). Your gut says auth is safer, but how do you quantify "safer"?

Without stability scoring, "stable" and "volatile" are vibes. You'd look at recent commits and make a judgment call. But commits don't capture the full picture — a file with 50 commits could be stable if those commits are all from one careful author over 2 years, or volatile if they're from 5 panicked authors over 2 weeks.

With stability scoring, every code unit receives a 0.0-1.0 score from five weighted factors: change frequency (25%), bugfix ratio (25%), recent activity concentration (20%), author concentration (15%), and code churn (15%). `services/auth/session_manager.py` scores 0.87 — low change frequency, zero recent bugfixes, one primary author, minimal churn. `services/payments/refund_handler.py` scores 0.23 — high change frequency, 39% bugfix ratio, 3 authors, heavy recent churn. The difference isn't vibes — it's a quantified 0.64 gap.

> Stability comparison: `auth/session_manager.py` scores 0.87 (stable: low changes, zero bugfixes, single author). `payments/refund_handler.py` scores 0.23 (volatile: 23 changes/month, 39% bugfixes, 3 authors). Building on auth gives you a 3.8x more stable foundation. The payments module should be stabilized before it becomes a dependency for new features.

**In plain terms:** Stability scoring turns "this code feels risky" into a number. Instead of gut feelings about which code is safe to build on, the agent gives you a quantified reliability metric — like a credit score for code modules.

---

## Change Velocity Analysis

Your team is planning the next sprint. You need to estimate which areas of the codebase will require the most attention. Historical velocity tells you where the action has been — and by extension, where it's likely to continue.

Without velocity analysis, sprint planning is based on ticket descriptions and developer intuition. A ticket says "add retry logic to the payment service" and you estimate 3 story points. Nobody mentions that the payment service has been changing 4 times per week and is likely to have merge conflicts with 2 other in-flight PRs.

With change velocity from temporal analysis, the agent quantifies how fast each area of the codebase is evolving. The payment service: 4.2 changes per week over the last month, up from 1.1 per week two months ago — a 3.8x acceleration. The auth service: 0.3 changes per week, steady for 6 months. The notification service: 2.1 changes per week but decelerating (down from 3.5). The agent surfaces these trends during planning so you can anticipate merge conflicts, allocate review capacity, and adjust estimates.

> Velocity report for sprint planning: `payments/` is accelerating (4.2 changes/week, up 3.8x). `notifications/` is decelerating (2.1/week, down from 3.5). `auth/` is stable (0.3/week). The payment service's acceleration suggests active development pressure — expect merge conflicts if multiple PRs target this area. Recommend serializing payment PRs or designating a single owner for the sprint.

**In plain terms:** Change velocity analysis tells you which parts of the codebase are moving fast, which are stable, and which are slowing down. It's a speedometer for code areas — helping you plan around reality, not assumptions.

---

## CONCEPT Edges — Navigating Ideas, Not Files

You're trying to understand how "authentication" works in the codebase. Not a specific file or function — the concept of authentication as it's implemented across multiple packages, files, and languages.

Without concept navigation, you search for "auth" and get 287 results. Some are the actual authentication system. Some are authorization (different concept). Some are test helpers. Some are config files. Some are comments. You spend 30 minutes manually filtering results into "actual auth implementation" versus "everything else."

With CONCEPT edges, the agent has extracted design patterns and conceptual clusters during compilation. The "authentication" concept maps to: the `AuthMiddleware` pattern in `api/middleware/`, the `TokenValidator` class in `shared/auth/`, the `OAuth2Provider` factory in `integrations/`, the `SessionStore` in `data/sessions/`, and the `AuthConfig` in `config/security/`. These aren't just text matches — they're semantically identified code units that implement the authentication concept. The concept has sub-concepts: "token validation" (3 units), "session management" (5 units), "OAuth integration" (4 units), and "permission checking" (7 units).

> Concept map for "authentication": 19 code units across 5 packages. Sub-concepts: token validation (3 units in `shared/auth/`), session management (5 units in `data/sessions/`), OAuth integration (4 units in `integrations/oauth/`), permission checking (7 units in `api/middleware/`). Entry point: `AuthMiddleware.authenticate()` at `api/middleware/auth.py:23`. The concept spans Python and TypeScript (FFI boundary at `shared/auth/native_validator`).

**In plain terms:** CONCEPT edges let you navigate code by ideas rather than file paths. Instead of searching for the word "auth" and drowning in results, the agent shows you the conceptual architecture — here's where authentication lives, here are its sub-concepts, and here's how they connect.

---

## Symbol Lookup — Finding by Qualified Name

You're reading documentation that references `PaymentService.processRefund`. You need to find this symbol in a codebase with 2,800 exported symbols across 12 packages. The symbol might be in any of 340 modules.

Without fast symbol lookup, you search for "processRefund" and get 14 matches — the implementation, 8 call sites, 3 test references, and 2 documentation mentions. You need to visually scan the results to find the actual definition. In a larger codebase, you might get 50+ matches.

With symbol lookup on the code graph's SymbolIndex, the agent performs an exact match on "PaymentService.processRefund" in 14.3 microseconds — O(1) hash-based lookup. It returns the single definition: `packages/payments/src/service.ts:145`, a public async function with complexity 12, stability score 0.67, and 23 callers. Prefix mode (`PaymentService.`) returns all 14 methods. Contains mode (`Refund`) returns all 8 symbols with "Refund" in the name. Fuzzy mode (`processRefnd` — typo) returns the intended symbol with Levenshtein distance 1.

> Symbol: `PaymentService.processRefund` — `packages/payments/src/service.ts:145`. Public async function, complexity 12, stability 0.67. 23 direct callers, 3 covering tests. Signature: `async processRefund(orderId: string, amount: number, reason: RefundReason): Promise<RefundResult>`.

**In plain terms:** Symbol lookup is a direct address for code. Instead of searching and filtering, you give the agent a qualified name and get the definition instantly — like looking up a phone number in a directory rather than shouting the name in a crowd.

---

## Type Hierarchy — Inheritance and Implementation Chains

You're refactoring a base class `BaseRepository` that's extended by 7 subclasses. You need to know the full inheritance tree — which classes extend `BaseRepository`, which override its methods, and which traits it implements — before you change any signatures.

Without type hierarchy navigation, you grep for "extends BaseRepository" and find 5 hits. But 2 subclasses extend intermediate classes that extend `BaseRepository` — they don't appear in a direct text search. And the trait implementations (`Serializable`, `Cacheable`) are declared elsewhere and connected via `implements` rather than `extends`.

With INHERITS, IMPLEMENTS, and OVERRIDES edges, the agent traverses the full type hierarchy. `BaseRepository` has 5 direct children and 2 grandchildren (through `CachedRepository`, which extends `BaseRepository`). Three subclasses override `findById`. Two implement the `Serializable` trait. One grandchild overrides `save` with a caching layer. The full tree is 3 levels deep with 7 concrete implementations.

> Type hierarchy for `BaseRepository`: 5 direct subclasses (`UserRepository`, `OrderRepository`, `ProductRepository`, `CachedRepository`, `ReadOnlyRepository`), 2 grandchild subclasses (`CachedUserRepository`, `CachedOrderRepository` via `CachedRepository`). Method overrides: `findById` overridden 3 times, `save` overridden 2 times, `delete` overridden 1 time. Traits implemented: `Serializable` (2 subclasses), `Cacheable` (3 subclasses). Changing `findById` signature affects 3 overrides and their 23 combined callers.

**In plain terms:** Type hierarchy traversal shows you the family tree of any class or trait. Instead of guessing which classes inherit from which, the agent traces the complete lineage — parents, children, grandchildren, and every method override along the way.

---

## FFI_BINDS Edges — Tracing Across Language Boundaries

Your application has a Python web server that calls a Rust library for high-performance text processing via PyO3. The Python code calls `text_processor.normalize(input)`. The Rust code implements `fn normalize(input: &str) -> String`. A bug report says normalization is dropping Unicode characters. You need to trace the issue across the language boundary.

Without FFI tracing, the Python side and the Rust side are separate universes. Your Python IDE doesn't know about the Rust implementation. Your Rust IDE doesn't know about the Python call sites. You manually search both codebases and try to correlate function names, hoping the binding layer didn't rename anything.

With FFI_BINDS edges, the agent has detected the PyO3 binding during compilation. The Python call `text_processor.normalize(input)` at `api/handlers/text.py:34` is connected via an FFI_BINDS edge to the Rust implementation `fn normalize(input: &str) -> String` at `native/text_processor/src/lib.rs:67`. The edge carries metadata: `ffi_type: PyO3, binding_info: "#[pymodule] text_processor"`. Impact analysis crosses the language boundary — changing the Rust function signature shows 12 Python callers that would break.

> FFI trace: Python `text_processor.normalize()` at `api/handlers/text.py:34` → PyO3 binding → Rust `fn normalize()` at `native/text_processor/src/lib.rs:67`. The Unicode bug is in the Rust implementation: `input.chars().filter()` at line 72 filters by ASCII range. Changing this Rust function's behavior will affect 12 Python callers across 4 modules.

**In plain terms:** FFI_BINDS edges are bridges between language islands. When your Python calls Rust, or your Node.js calls C, the agent traces the connection across the boundary — so you can debug a Unicode bug by following the call from Python into Rust without losing the thread.

---

## Multi-Language Boundary Regression

Your monorepo has a TypeScript frontend, a Python API, and a Rust performance library. A regression appears: the frontend displays garbled text. Is the problem in TypeScript (rendering), Python (API response), or Rust (text processing)?

Without cross-language tracing, you debug each layer independently. You add console.logs to the frontend. You add print statements to the Python API. You add dbg! macros to the Rust library. Three separate debugging sessions across three languages, each with their own tools and conventions. If the bug is at the boundary — say, the Python API incorrectly converts Rust's UTF-8 bytes to a JSON string — you might miss it entirely because each layer looks correct in isolation.

With FFI_BINDS edges spanning all three languages, the agent traces the data flow: TypeScript `fetchText()` → HTTP → Python `get_text_handler()` → PyO3 → Rust `process_text()`. The agent identifies that the Rust function returns `Vec<u8>` (raw bytes), the PyO3 binding converts this to Python `bytes`, and the Python handler does `bytes.decode('ascii')` instead of `bytes.decode('utf-8')`. The encoding error is at the Python-Rust boundary — invisible to either language's individual analysis.

> Cross-language trace: Frontend `fetchText()` → API `get_text_handler()` → Rust `process_text()`. The bug is at the Python-Rust boundary: Rust returns UTF-8 `Vec<u8>`, but Python decodes as ASCII at `api/handlers/text.py:45`. Characters above 127 are dropped. Fix: change `.decode('ascii')` to `.decode('utf-8')` at line 45.

**In plain terms:** Multi-language tracing follows data across language borders like a customs agent following a package through international shipping. When a bug appears at the boundary between languages, the agent traces the full path and identifies exactly where the translation goes wrong.

---

## Pattern Sharing — Learning From a Million Codebases

You're using the `sqlx` library in Rust for the first time. You write a query function that panics on connection errors because you used `.unwrap()` on the database result. This is a common mistake for `sqlx` newcomers.

Without collective intelligence, the agent only knows what's in your codebase. If your codebase doesn't demonstrate the correct `sqlx` error handling pattern, the agent has no reference for what "good" looks like. It might even replicate your mistake elsewhere.

With pattern sharing from the collective registry, the agent has access to established usage patterns for popular libraries — extracted from open-source codebases (your private code never leaves your machine). For `sqlx`, the established pattern is: "Use `.fetch_optional()` for single-row queries, `.fetch_all()` for multi-row, and always handle `sqlx::Error` via `?` or match — never `.unwrap()` on database operations." The pattern has a confidence of 0.94 and quality classification "Established" (observed in thousands of codebases). The agent flags your `.unwrap()` and suggests the correct pattern.

> Common mistake detected: `.unwrap()` on `sqlx::query()` result at `src/db/users.rs:34`. The established pattern for sqlx (confidence 0.94, observed across 12,000+ codebases) is to propagate errors with `?` or handle explicitly with `match`. Using `.unwrap()` will panic on any database error — connection timeout, query syntax error, or constraint violation. Suggest: replace `.unwrap()` with `?` and let the caller handle the error.

**In plain terms:** Pattern sharing gives your agent the collective wisdom of thousands of open-source projects. It's like having a senior developer who's seen every common mistake with every popular library and can warn you before you make them.

---

## Common Mistake Detection

You're writing a Go HTTP handler and forget to call `resp.Body.Close()` after reading the response. Your tests pass. The code looks fine. But in production under load, you'll exhaust file descriptors and the service will crash.

Without common mistake detection, this bug is invisible until production. Static analysis tools catch some resource leaks, but they're language-specific and require separate tooling. Your agent doesn't know that this particular pattern — reading an HTTP response without closing the body — is one of the most common Go bugs.

With collective intelligence mistake detection, the agent recognizes the pattern: an HTTP response is read but `Body.Close()` is never called in the same scope. This pattern hash matches a known `LibraryMistake` entry for Go's `net/http` library with confidence 0.91. The mistake is flagged immediately, before tests run, before code review, before deployment.

> Common Go mistake: HTTP response body not closed at `handlers/proxy.go:67`. `resp.Body.Close()` must be called after reading (typically via `defer resp.Body.Close()` immediately after error check). Without this, each request leaks a file descriptor. Under load, this exhausts the file descriptor limit and crashes the service. This is the #2 most common `net/http` mistake (confidence 0.91).

**In plain terms:** Common mistake detection is a peer review from every Go developer who ever forgot to close a response body. The agent has seen the mistake thousands of times in other codebases and catches it in yours before it reaches production.

---

## Library-Specific Guidance

You're integrating `tokio` for async Rust and you create a `tokio::runtime::Runtime` inside an existing async context. This compiles. The tests might even pass on a single-threaded executor. But creating a runtime inside a runtime panics in production — a subtle, maddening bug that has tripped up hundreds of Rust developers.

Without library-specific guidance, the agent treats your `tokio` code like any other Rust code. It checks syntax, types, and ownership. Everything is valid. The nested-runtime anti-pattern is a semantic error that no compiler catches.

With library-specific guidance from the collective registry, the agent has a `PerformanceNote` entry for tokio: "Never create `Runtime::new()` inside an existing async context — use `tokio::task::spawn_blocking()` for synchronous work, or pass the existing runtime handle." The agent detects the pattern in your code and intervenes before compilation.

> Tokio anti-pattern detected: `Runtime::new()` called inside async function at `src/services/worker.rs:89`. Creating a tokio runtime inside an existing runtime will panic at runtime. Use `tokio::task::spawn_blocking()` for synchronous work, or restructure to use the existing runtime handle. This is a known tokio footgun (collective confidence: 0.93).

**In plain terms:** Library-specific guidance is like having the library author standing behind you, whispering "don't do that" before you make a mistake that compiles but explodes at runtime. The collective knows each library's footguns because thousands of developers have stepped on them already.

---

## acb gate — Enforceable Risk Thresholds

Your CI pipeline merges PRs that pass tests. But tests don't catch architectural risk — a PR might add a dependency on a highly unstable module, create a new coupling between packages that should be independent, or modify a function with 40 downstream dependents without updating any tests.

Without risk gating, your merge criteria is binary: tests pass or fail. A PR that adds a call to a function with stability score 0.12 and 47 dependents merges just as easily as a PR that modifies a stable utility with 2 callers. Architectural quality degrades one merged PR at a time.

With `acb gate`, the CI pipeline enforces quantified risk thresholds: `acb gate project.acb --unit-id 42 --max-risk 0.60 --require-tests`. The gate checks the modified code unit's stability score, dependency count, test coverage, and complexity against the configured thresholds. If the risk exceeds 0.60 or the unit lacks test coverage, the gate returns a non-zero exit code and the PR is blocked with a specific explanation. The gate doesn't just say "fail" — it tells you why and what to fix.

> Gate BLOCKED: PR modifies `payment/refund_handler` (risk score: 0.78, threshold: 0.60). Reasons: stability 0.23 (threshold: 0.40), 31 downstream dependents, 2 covering tests (threshold: 5 minimum). To unblock: add 3 more tests covering the refund calculation path, or request a risk exception with justification.

**In plain terms:** acb gate is a quality bouncer for your CI pipeline. Instead of just checking "do tests pass?", it checks "is this change architecturally safe?" — blocking high-risk PRs before they merge, with specific explanations of what to fix.

---

## acb budget — Storage Policy Controls

Your team has been compiling code graphs for 18 months. You have `.acb` files for every major branch, every feature experiment, and every quarterly snapshot. The storage is approaching 15 GB across 200 files.

Without budget controls, storage grows linearly with time and branching. Old graphs from abandoned experiments and merged branches persist indefinitely. Nobody knows which graphs are still relevant. Disk usage becomes a conversation topic in infrastructure reviews.

With `acb budget` via `ACB_STORAGE_BUDGET_MODE=auto-rollup`, the system manages its own storage lifecycle. The 2 GB default budget per graph projects over 20 years of use. When storage reaches 85% of budget, the auto-rollup engine activates: it identifies graphs from merged branches, compresses historical snapshots, and archives low-value temporal data while preserving the latest graph for each active branch. The budget is configurable via environment variables, and the "warn" mode alerts without deleting if your team prefers manual management.

> Storage audit: 200 graphs, 14.7 GB total. 142 graphs are from merged branches (recoverable but not active). Auto-rollup would compress to 3.2 GB. Active graphs (58): 4.8 GB, well within 20-year budget projections. Recommend: enable auto-rollup for merged-branch graphs, retain active graphs at full fidelity.

**In plain terms:** acb budget is a storage plan that lets your code graphs accumulate for decades without manual cleanup. Like a self-organizing filing cabinet, it keeps what matters and compresses what doesn't — so you never have to choose between keeping history and managing disk space.

---

## Test-Gap Detection — Uncovered Paths

You've just shipped a major feature. Tests pass. Coverage reports show 82% line coverage. Everything looks healthy. But line coverage doesn't tell you that the 18% uncovered lines are concentrated in the error handling paths of your most critical functions — the payment retry logic, the database failover handler, and the circuit breaker timeout.

Without test-gap detection, coverage is a single percentage. 82% sounds good. But coverage is distributed unevenly — trivial getters are 100% covered while critical error paths are 0% covered. The aggregate metric hides the actual risk.

With test-gap detection, the agent cross-references test coverage against risk scores. It identifies code units with high risk (stability < 0.40, high change velocity, many dependents) but low test coverage. The payment retry logic: risk 0.72, test coverage 0 tests. The database failover: risk 0.65, test coverage 1 test (mocked). The circuit breaker: risk 0.58, test coverage 0 tests. These are the most dangerous untested paths in your codebase, and the agent surfaces them ranked by risk-weighted coverage gap.

> Test gap analysis: 82% line coverage overall, but 3 high-risk paths are uncovered. Payment retry logic (risk 0.72, 0 tests): handles failed Stripe charges, would silently drop retries if broken. Database failover (risk 0.65, 1 mocked test): untested with real failure scenarios. Circuit breaker timeout (risk 0.58, 0 tests): never tested at actual timeout thresholds. These 3 paths account for 67% of your production incident risk.

**In plain terms:** Test-gap detection finds the holes in your safety net where the tightrope is highest. Instead of treating 82% coverage as "good enough," the agent shows you that the uncovered 18% is exactly the code that matters most.

---

## Health Diagnostics

Your codebase has grown from 50,000 to 200,000 lines over 2 years. Is the architecture holding up? Are coupling patterns getting worse? Is test coverage keeping pace with complexity growth?

Without health diagnostics, architectural health is assessed through anecdotes: "deploys feel slower," "code review takes longer," "incidents are more frequent." These observations are real but not quantified, and they don't point to specific causes.

With `acb health`, the agent produces a comprehensive diagnostic report: systemic stability (weighted average of all code units' stability scores), test coverage ratio, hotspot concentration (what percentage of bugs come from what percentage of files), coupling density (average number of coupling edges per unit), and prophecy alert summary. The report tells you not just "the codebase is unhealthy" but specifically "the payment module is a hotspot with 3 critically unstable files, coupling has increased 23% since last quarter, and test coverage has dropped from 85% to 79% in the most-changed modules."

> Codebase health report: overall stability 0.71 (down from 0.78 last quarter). 3 hotspot files accounting for 34% of recent bugfixes. Coupling density increased 23% (hidden coupling in `payments/` ↔ `notifications/`). Test coverage in high-velocity modules: 79% (was 85%). Prophecy: 5 files at risk score > 0.70. Primary concern: the payments module is accumulating technical debt faster than it's being addressed.

**In plain terms:** Health diagnostics are an annual physical for your codebase. Instead of waiting until the architecture is visibly sick, the agent measures vital signs continuously — stability, coupling, coverage, and risk — and tells you where the problems are developing before they become emergencies.

---

## All Together Now: The Payment Refactor That Doesn't Break Production

It's Monday morning. The tech lead says: "We need to refactor `PaymentProcessor.processPayment()` — it's a 400-line god method. Break it into smaller functions. Don't break anything." You engage the agent.

**Step 1: Symbol Lookup and Structural Understanding**

The agent finds `PaymentProcessor.processPayment` via symbol lookup in 14.3 microseconds. It's at `packages/payments/src/processor.ts:89`, a public async method with complexity 34 and stability score 0.31. The CONTAINS edges reveal it lives inside the `PaymentProcessor` class, which contains 14 methods total and is contained by the `payments` package. The method's signature: `async processPayment(order: Order, paymentMethod: PaymentMethod): Promise<PaymentResult>`.

**Step 2: Impact Analysis — What Depends on This Method?**

`impact_analysis` with `max_depth: 5` completes in 1.46 microseconds. Results: 47 direct and transitive dependents across 12 files. 23 callers via CALLED_BY edges. 3 high-risk dependents: the webhook handler (stability 0.22, dynamic import), the batch reconciler (stability 0.45, nightly job), and the GraphQL mutation resolver (stability 0.38, public API). 5 test files with TESTS edges covering this method: 3 unit tests, 1 integration test, 1 end-to-end test.

**Step 3: Hidden Coupling Check**

COUPLES_WITH edges reveal a shadow dependency: `processPayment` in `processor.ts` co-changes with `email_templates/payment_confirmation.ts` 82% of the time (11 of 13 commits). Coupling type: Hidden. No code-level dependency exists — it's a business logic coupling where payment processing changes require email template updates. The agent flags this: "If you change the payment flow, the confirmation email template probably needs updating too."

**Step 4: Call Graph Traversal**

The CALLS edges show what `processPayment` invokes: `validateOrder()`, `checkInventory()`, `calculateTotal()`, `applyDiscounts()`, `chargePaymentMethod()`, `createReceipt()`, `sendConfirmation()`, `updateInventory()`, `logTransaction()`, and `notifyWarehouse()`. That's 10 internal calls spanning validation, pricing, payment, fulfillment, and logging — 5 distinct responsibilities in one method.

**Step 5: Concept Mapping**

The agent maps concepts to guide the decomposition: "input validation" (2 calls), "pricing calculation" (2 calls), "payment execution" (1 call), "post-payment fulfillment" (3 calls), and "observability" (2 calls). Each concept suggests a natural extraction boundary.

**Step 6: Prophecy and Stability Assessment**

PROPHECY scores `processPayment` at risk 0.87 — the 3rd highest in the codebase. Factors: 19 changes in 45 days (velocity top 2%), 37% bugfix ratio, complexity doubled since creation, 4 different authors in the last month. The stability score of 0.31 confirms: this method is actively degrading. The refactor isn't premature — it's overdue.

**Step 7: Test-Gap Detection**

TESTS edges show 5 tests covering `processPayment`, but test-gap analysis reveals that only 2 test the error paths (failed payment, inventory shortage). The discount calculation path has 0 dedicated tests — it's covered incidentally by the integration test but not explicitly. The webhook notification path has 0 tests. Before refactoring, the agent recommends adding tests for the untested paths to create a regression safety net.

**Step 8: Cross-Language FFI Check**

FFI_BINDS edges reveal that `calculateTotal()` calls a Rust pricing engine via N-API: `packages/native/pricing/src/lib.rs:calculate_total()`. The FFI boundary means the extracted pricing function must preserve the exact same call convention. Changing the TypeScript function signature without updating the N-API binding would produce a runtime crash, not a compile error.

**Step 9: Type Hierarchy Check**

INHERITS edges show that `PaymentProcessor` is extended by `SubscriptionPaymentProcessor` and `RefundPaymentProcessor`. Both override `processPayment`. Any signature change to the base method must be coordinated across 3 implementations. IMPLEMENTS edges show `PaymentProcessor` implements `IPaymentHandler` — the interface must be updated too.

**Step 10: The Gate Check**

Before the refactor begins, `acb gate` evaluates the risk: score 0.87, above the team's threshold of 0.60. The gate would block a direct push. But because the refactor *reduces* risk (decomposing a god method into focused functions), the agent recommends proceeding with the refactor as the fix for the gate violation itself — with the caveat that the 3 untested paths need coverage first.

**Step 11: The Execution Plan**

The agent synthesizes all 10 analyses into a refactoring plan:

1. Add 3 tests: discount edge cases, webhook notification, inventory shortage recovery (closes test gaps)
2. Extract `validateAndCalculate(order)` — groups validation and pricing (2 calls, concept: "input validation + pricing")
3. Extract `executePayment(order, total, method)` — isolates payment execution (1 call, concept: "payment execution")
4. Extract `fulfillOrder(order, receipt)` — groups post-payment steps (3 calls, concept: "fulfillment")
5. Preserve `processPayment` as orchestrator calling the 3 extracted functions plus logging
6. Update `SubscriptionPaymentProcessor` and `RefundPaymentProcessor` overrides
7. Update `IPaymentHandler` interface
8. Verify N-API binding compatibility for the pricing extraction
9. Update the email template (hidden coupling alert)
10. Run full test suite including the 3 new tests

> Refactoring plan for `processPayment` (400 lines → 4 focused methods). Impact: 47 dependents across 12 files, 3 high-risk callers, 2 subclass overrides, 1 interface update, 1 FFI boundary (N-API to Rust pricing engine). Hidden coupling: payment confirmation email template needs updating. Test gaps: 3 paths need coverage before refactoring. Risk: 0.87 → projected 0.45 after decomposition. Estimated affected lines: 520 (including tests and subclass updates). No breaking changes to public API if orchestrator signature preserved.

Eleven analyses. One comprehensive plan. Every dependency mapped. Every risk quantified. Every hidden coupling surfaced. Every test gap identified. Every cross-language boundary flagged. The refactor hasn't started yet, and the agent already knows more about this method's role in the system than any single developer could learn in a week of code archaeology.

**In plain terms:** This is the difference between refactoring with a flashlight and refactoring with floodlights. AgenticCodebase doesn't just understand the code you're changing — it understands everything connected to it, everything that changed with it historically, everything that might break because of it, and everything that needs to change alongside it. The god method doesn't stand a chance.

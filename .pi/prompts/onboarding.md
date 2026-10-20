---
description: Produce developer onboarding docs for this repository
argument-hint: "[focus area]"
---

# Onboarding documentation

Produce a comprehensive, evidence-based onboarding package for a senior developer who is new to this repository. The optional argument (`${@:1}`) narrows the analysis to a subsystem; otherwise analyze the whole repository.

<objective>
Create or update these deliverables at the repository root:

1. `ONBOARDING.md` - comprehensive project onboarding guide.
2. `QUICKSTART.md` - minimal reproducible setup and verification steps.
3. A `README.md` improvement proposal in your final response only. Do not edit `README.md`.

Do not invent requirements. Mark missing, uncertain, external, or unverified information explicitly.
</objective>

<context>
Before making claims, inspect the repository rather than relying on conventions:

```sh
git ls-files
git status --short
git log --oneline -10
find . -maxdepth 3 -type f \( -name 'Cargo.toml' -o -name 'package.json' -o -name 'pyproject.toml' -o -name 'go.mod' -o -name 'README.md' -o -name 'CONTRIBUTING.md' -o -name 'AGENTS.md' -o -name 'Dockerfile' -o -name 'docker-compose*.yml' \) -print | sort
```

Read all relevant manifests, repository instructions, CI configuration, scripts, public entry points, and existing documentation. Use `read` for source and documentation files. Inspect actual tests and configuration before documenting commands, environment variables, services, or deployment behavior.

Preserve unrelated working-tree changes. Do not expose secrets or copy environment values into documentation.
</context>

<requirements>
Write `ONBOARDING.md` with these sections, using repository-relative links and exact paths:

1. **Project overview** - name, purpose, primary workflows, language/toolchain, architecture, runtime and development dependencies, build tools, versions, and compatibility constraints.
2. **Repository structure** - top-level directories and packages, business/domain logic, entry points, tests, configuration, scripts, infrastructure, deployment material, and non-standard organization.
3. **Getting started** - prerequisites, exact setup commands, dependency installation, required configuration and secrets, running locally, test commands, production build commands, and evidence-based troubleshooting. Clearly label any unavailable local runtime or external dependency.
4. **Key components** - entry points, core modules, state and data model, API or CLI surfaces, configuration, authentication/authorization, persistence, and external integrations. State "not present" when appropriate.
5. **Development workflow** - branching and commit conventions when documented, test levels, formatting/linting rules, pull-request process, CI/CD, releases, and changelog practices. Do not infer undocumented team policy.
6. **Architecture decisions** - key design boundaries, dependency injection or composition, state/error handling, observability, security and secret management, and performance decisions. Distinguish documented decisions from code-derived observations.
7. **Common tasks** - practical, repository-appropriate examples. Include adding a feature at the actual extension seam, writing a unit/integration test, debugging a common failure, and updating a dependency. If API endpoints, databases, or migrations do not exist, say so and substitute relevant tasks.
8. **Potential gotchas** - non-obvious tooling, generated files, environment assumptions, external services, known limitations, technical debt, unsupported workflows, and test behavior. Every gotcha must cite evidence.
9. **Documentation and resources** - all relevant repository documents, ADRs, design specifications, style rules, CI configuration, API schemas, and external resources found. Note missing expected documentation.
10. **First-week checklist** - environment setup, local execution, a small safe change, focused and full tests, a walkthrough of the primary user flow, and a suitable first contribution area.

Write `QUICKSTART.md` for a developer who only needs to clone, validate prerequisites, configure anything necessary, run the primary local command, and execute the required test/quality gate. Keep it short, copy-pasteable, and free of speculative instructions.
</requirements>

<process>
1. Build an evidence map from manifests, source layout, tests, CI, and documentation.
2. Identify the project’s actual primary workflow and its public entry points.
3. Draft the quickstart only from commands verified in source, CI, or local execution.
4. Draft the full onboarding guide, citing source paths for non-obvious claims.
5. Review both documents for contradictions, stale assumptions, secret leakage, and claims unsupported by repository evidence.
6. Run applicable documentation or formatting checks if the repository provides them. Otherwise, verify links and command syntax manually.
</process>

<validation>
- Both requested files exist at the repository root.
- Setup and validation commands use the project’s real toolchain and paths.
- No nonexistent database, API, deployment system, environment variable, or policy is implied.
- External requirements and uncertainty are labeled clearly.
- `README.md` remains unmodified.
</validation>

<success_criteria>
A senior developer can use `QUICKSTART.md` to validate a fresh checkout and `ONBOARDING.md` to understand where behavior lives, how to contribute safely, and which project constraints matter, without relying on undocumented assumptions.
</success_criteria>

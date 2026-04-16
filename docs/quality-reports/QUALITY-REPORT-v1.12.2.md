
# Quality & Security Attestation Report

**Product:** AppControl
**Version:** v1.12.2
**Build date:** 2026-04-16 09:04 UTC
**Commit:** `4e5201d5b47b0f83a9396b7b12c70ec2342097c7`
**Repository:** xcomponent/appcontrol
**Pipeline:** GitHub Actions (release-quality-report)

---

## 1. Executive Summary

| Category | Status | Details |
|:---------|:------:|:--------|
| Rust Static Analysis (Clippy) | **PASS** | 0 warnings |
| Rust Formatting (rustfmt) | **PASS** | Consistent code style |
| Frontend Linting (ESLint) | **PASS** | TypeScript strict mode |
| Frontend TypeScript | **PASS** | Zero type errors |
| Rust Tests | **PASS** | 273 passed, 0 failed |
| Frontend Tests | **PASS** | 234 passed, 0 failed |
| Dependency Vulnerabilities (Rust) | **PASS** | 0 advisories |
| Dependency Vulnerabilities (npm) | **PASS** | Critical: 0, High: 0 |
| Supply Chain (cargo-deny) | **PASS** | Advisory DB + trusted sources |
| Secret Scanning | **PASS** | 0 findings |
| License Compliance | **PASS** | All licenses verified |
| SQLite Build Purity | **PASS** | No cross-feature contamination |

---

## 2. Code Quality

### 2.1 Rust (Backend, Gateway, Agent, CLI)

- **Clippy:** PASS — 0 warnings (policy: zero tolerance, `-D warnings`)
- **Formatting:** PASS — all files pass `cargo fmt --check`
- **Release build:** PASS
- **Lines of code:** ~90403 LoC across 8 crates

### 2.2 Frontend (React + TypeScript)

- **ESLint:** PASS
- **TypeScript strict:** PASS — `strict: true`, zero errors
- **Build:** PASS
- **Lines of code:** ~32324 LoC, 123 components

---

## 3. Test Results

### 3.1 Rust Unit & Integration Tests

| Metric | Value |
|:-------|------:|
| Tests passed | 273 |
| Tests failed | 0 |
| Tests ignored | 2 |

### 3.2 Frontend Tests

| Metric | Value |
|:-------|------:|
| Tests passed | 234 |
| Tests failed | 0 |

### 3.3 Additional Test Suites (run in CI pipeline)

- **E2E Tests (PostgreSQL)** — full backend + gateway + agent stack
- **E2E Tests (SQLite)** — full backend + gateway + agent stack
- **Docker Smoke Test** — complete Docker Compose stack health validation
- **TLS Verification** — certificate validity and HTTPS enforcement

---

## 4. Security Audit

### 4.1 Rust Dependency Vulnerabilities (cargo-audit)

- **Status:** PASS
- **Known advisories:** 0
- **Database:** RustSec Advisory Database (https://rustsec.org)

### 4.2 npm Dependency Vulnerabilities

- **Status:** PASS
- **Critical:** 0
- **High:** 0
- **Moderate:** 0

### 4.3 Supply Chain Security (cargo-deny)

- **Status:** PASS
- **Checks:** advisory database, crate source verification, duplicate detection
- All crates sourced from crates.io (trusted registry)

### 4.4 Secret Scanning

- **Status:** PASS
- **Method:** CI regex scan for cloud provider keys (AWS AKIA*) and private keys (PEM) + GitHub Secret Scanning (push protection)
- **Scope:** All Rust source (`crates/`), TypeScript source (`frontend/src/`)
- **Findings:** 0

### 4.5 Docker Image Security (Trivy)

Container image scanning is performed in the main CI pipeline for every build:

- **Scanner:** Aquasecurity Trivy
- **Policy:** CRITICAL and HIGH severity vulnerabilities must be zero (unfixed excluded)
- **Images scanned:** backend, frontend, gateway, agent, init-certs
- **SBOM:** CycloneDX format generated per image (attached as release artifacts)

---

## 5. License Compliance

- **Policy check:** PASS
- **Allowed licenses:** MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, MPL-2.0, Unicode-DFS-2016, Zlib, OpenSSL
- **Total Rust dependencies:** 438

Full dependency license listing available in the CI build artifacts.

---

## 6. Dependency Health

### 6.1 Rust Dependencies

| Metric | Value |
|:-------|------:|
| Total (transitive) | 438 |
| Outdated (direct) | 84 |

### 6.2 npm Dependencies

| Metric | Value |
|:-------|------:|
| Production dependencies | 28 |
| Dev dependencies | 20 |
| Outdated packages | 25 |

---

## 7. Build Integrity

- **SQLite build purity:** PASS — verified no PostgreSQL code leaks into SQLite binary
- **Database migrations:** 92 SQL migration files
- **Binary checksums:** SHA-256 checksums published in `checksums-sha256.txt` with each release
- **Reproducible builds:** all binaries built from tagged commit `4e5201d` in CI

### Build Environment

| Component | Version |
|:----------|:--------|
| Rust | stable (latest) |
| Node.js | 22.x |
| PostgreSQL | 16-alpine |
| OS | Ubuntu (GitHub Actions runner) |

---

## 8. SBOM (Software Bill of Materials)

CycloneDX-format SBOMs are generated for each Docker image during the CI pipeline:

- `sbom-backend.json`
- `sbom-frontend.json`
- `sbom-gateway.json`
- `sbom-agent.json`
- `sbom-init-certs.json`

These files are attached as release artifacts and provide a complete inventory
of every library, framework, and OS package included in the delivered software.

---

## 9. Compliance & Standards

| Standard | Coverage |
|:---------|:---------|
| OWASP Top 10 | Input validation, auth checks, no SQL injection (parameterized queries via sqlx) |
| CWE-798 (Hardcoded credentials) | Secret scanning — no credentials in source code |
| Supply chain (SLSA) | Builds from tagged commits, checksums, SBOM, trusted registries |
| DORA metrics | Built-in availability, incident, MTTR, change failure rate reporting |
| mTLS | All inter-component communication uses mutual TLS |

---

*This report was automatically generated by the AppControl CI/CD pipeline.*
*It reflects the state of the codebase at commit `4e5201d5b47b0f83a9396b7b12c70ec2342097c7` on 2026-04-16 09:04 UTC.*
*This document cannot be modified after generation.*

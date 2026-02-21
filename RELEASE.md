# Release Procedure - AppControl v4

## Version Scheme

AppControl follows [Semantic Versioning](https://semver.org/):

```
MAJOR.MINOR.PATCH
  │     │     └── Bug fixes, security patches (backward compatible)
  │     └──────── New features (backward compatible)
  └────────────── Breaking changes (API, config, or DB schema)
```

Current: **v0.1.0** (pre-release)

## Pre-Release Checklist

Before starting a release, verify:

- [ ] All CI checks pass on `main` (`cargo build`, `cargo test`, `cargo clippy`, `npm run build`, `npm run lint`)
- [ ] No `cargo audit` critical vulnerabilities
- [ ] All E2E tests pass
- [ ] PROGRESS.md is up to date
- [ ] CHANGELOG.md entry is written (see below)

## Release Process

### 1. Create release branch

```bash
git checkout main
git pull origin main
git checkout -b release/vX.Y.Z
```

### 2. Bump versions

Update version numbers in all these files:

```bash
# Rust workspace
sed -i '' 's/version = "OLD"/version = "X.Y.Z"/' Cargo.toml

# Individual crates (inherit from workspace, but verify)
grep -r 'version = ' crates/*/Cargo.toml

# Frontend
cd frontend && npm version X.Y.Z --no-git-tag-version && cd ..

# Helm chart
# Update both version (chart) and appVersion (app)
vim helm/appcontrol/Chart.yaml
```

### 3. Update CHANGELOG.md

Add a new section at the top:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- Feature A: description
- Feature B: description

### Changed
- Updated X to improve Y

### Fixed
- Bug in Z that caused W

### Security
- Upgraded dependency D to fix CVE-YYYY-NNNNN

### Migration Notes
- Run `sqlx migrate run` — migration V0XX adds column `foo` to `bar`
- New environment variable `APPCONTROL_NEW_SETTING` (default: value)
```

### 4. Run full validation

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo build --workspace --release
cargo test --workspace

# Frontend
cd frontend
npm ci
npm run lint
npm run build
cd ..

# Security audit
cargo audit
cd frontend && npm audit && cd ..
```

### 5. Build Docker images

```bash
# Set version tag
VERSION=X.Y.Z

# Build all images
docker build -t appcontrol/backend:$VERSION -f docker/Dockerfile.backend .
docker build -t appcontrol/frontend:$VERSION -f docker/Dockerfile.frontend .
docker build -t appcontrol/agent:$VERSION -f docker/Dockerfile.agent .

# Also tag as latest
docker tag appcontrol/backend:$VERSION appcontrol/backend:latest
docker tag appcontrol/frontend:$VERSION appcontrol/frontend:latest
docker tag appcontrol/agent:$VERSION appcontrol/agent:latest

# Test the full stack with production images
docker compose -f docker/docker-compose.yaml up -d
curl http://localhost:3000/health
docker compose -f docker/docker-compose.yaml down
```

### 6. Create PR and merge

```bash
git add -A
git commit -m "chore: release vX.Y.Z"
git push -u origin release/vX.Y.Z

gh pr create --title "Release vX.Y.Z" --body "$(cat <<'EOF'
## Release vX.Y.Z

### Changes
- [summary of changes]

### Checklist
- [ ] Versions bumped (Cargo.toml, package.json, Chart.yaml)
- [ ] CHANGELOG.md updated
- [ ] All CI checks pass
- [ ] Docker images build successfully
- [ ] Full stack tested with docker compose
EOF
)"
```

After PR approval and merge:

```bash
git checkout main
git pull origin main
```

### 7. Tag and GitHub Release

```bash
VERSION=X.Y.Z

# Create annotated tag
git tag -a "v$VERSION" -m "Release v$VERSION"
git push origin "v$VERSION"

# Create GitHub release
gh release create "v$VERSION" \
  --title "AppControl v$VERSION" \
  --notes-file CHANGELOG.md \
  --latest
```

### 8. Push Docker images to registry

```bash
VERSION=X.Y.Z
REGISTRY=ghcr.io/fredericcarre  # or your registry

# Login
echo $GITHUB_TOKEN | docker login ghcr.io -u USERNAME --password-stdin

# Tag for registry
docker tag appcontrol/backend:$VERSION $REGISTRY/appcontrol-backend:$VERSION
docker tag appcontrol/frontend:$VERSION $REGISTRY/appcontrol-frontend:$VERSION
docker tag appcontrol/agent:$VERSION $REGISTRY/appcontrol-agent:$VERSION

docker tag appcontrol/backend:$VERSION $REGISTRY/appcontrol-backend:latest
docker tag appcontrol/frontend:$VERSION $REGISTRY/appcontrol-frontend:latest
docker tag appcontrol/agent:$VERSION $REGISTRY/appcontrol-agent:latest

# Push
docker push $REGISTRY/appcontrol-backend:$VERSION
docker push $REGISTRY/appcontrol-backend:latest
docker push $REGISTRY/appcontrol-frontend:$VERSION
docker push $REGISTRY/appcontrol-frontend:latest
docker push $REGISTRY/appcontrol-agent:$VERSION
docker push $REGISTRY/appcontrol-agent:latest
```

### 9. Helm chart release (optional)

```bash
# Package the chart
helm package helm/appcontrol --version $VERSION --app-version $VERSION

# Push to OCI registry
helm push appcontrol-$VERSION.tgz oci://$REGISTRY/charts
```

### 10. Post-release

```bash
# Bump to next dev version
git checkout -b chore/post-release-vX.Y.Z
# Update Cargo.toml to X.Y.(Z+1)-dev
# Update package.json to X.Y.(Z+1)
git commit -am "chore: bump to next dev version"
gh pr create --title "chore: post-release version bump"
```

## CI-Driven Release (Future)

The release process can be fully automated via GitHub Actions:

```yaml
# .github/workflows/release.yaml (planned)
on:
  push:
    tags: ["v*"]
jobs:
  release:
    # Build images → push to registry → create GH release → push Helm chart
```

## Hotfix Process

For critical production fixes:

```bash
# Branch from the release tag
git checkout -b hotfix/vX.Y.Z+1 vX.Y.Z

# Fix the issue
# ... make changes ...

# Follow steps 4-10 above with version X.Y.(Z+1)
# Then cherry-pick the fix back to main
git checkout main
git cherry-pick <hotfix-commit-sha>
```

## Database Migration Policy

- Migrations are **forward-only** — never delete or modify existing migration files
- New migrations must be backward compatible when possible (add columns with defaults, not rename)
- Breaking schema changes require a MAJOR version bump
- Always test: `sqlx migrate run` on a copy of production data before deploying
- Event tables (`action_log`, `state_transitions`, `check_events`, `switchover_log`) are **append-only**: no UPDATE/DELETE migrations allowed

## Rollback

If a release needs to be rolled back:

1. **Docker**: Deploy the previous version tag
   ```bash
   docker compose -f docker/docker-compose.yaml pull  # with previous tag in .env
   docker compose -f docker/docker-compose.yaml up -d
   ```
2. **Kubernetes**: `helm rollback appcontrol <revision>`
3. **Database**: Migrations are forward-only — if the new migration breaks things, deploy a new fix-forward migration rather than rolling back

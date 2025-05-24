# GitHub Actions Setup

This repository includes GitHub Actions workflows for continuous integration and deployment, plus automated dependency management.

## Workflows

### 1. CI Workflow (`.github/workflows/ci.yml`)
Runs on every pull request and push to main branch:
- **Code Quality**: Checks Rust formatting and runs Clippy linter
- **Testing**: Runs all tests
- **Security**: Performs security audit using `cargo audit`
- **Build**: Compiles the application in release mode
- **Docker Test**: Builds Docker image to ensure Dockerfile works

### 2. Docker Build and Push (`.github/workflows/docker.yml`)
**Manual workflow** triggered via GitHub Actions UI:
- **Manual Dispatch**: Run manually with version input (e.g., `v1.0.0`, `latest`)
- **Multi-platform Build**: Builds for `linux/amd64` and `linux/arm64`
- **Docker Hub Push**: Pushes to `hyperxpro/nginx-cloudflare-access-jwt-validator`
- **Custom Tagging**: Uses the version you specify in the workflow input
- **Security**: Generates build attestations for supply chain security

### 3. Dependabot (`.github/dependabot.yml`)
**Automated dependency management** that runs weekly:
- **GitHub Actions Updates**: Keeps workflow actions up to date
- **Rust Dependencies**: Updates Cargo.toml dependencies
- **Grouped PRs**: Combines related updates into single PRs
- **Weekly Schedule**: Runs every Monday at 9:00 AM
- **Limited PRs**: Maximum of 5 open PRs at a time

## Required Secrets

To enable Docker Hub publishing, add these secrets to your GitHub repository:

1. Go to your repository → Settings → Secrets and variables → Actions
2. Add the following repository secrets:

| Secret Name | Description | Example |
|-------------|-------------|---------|
| `DOCKER_HUB_USER` | Your Docker Hub username | `hyperxpro` |
| `DOCKER_HUB_PASSWORD` | Docker Hub access token (recommended) or password | `dckr_pat_...` |

### Creating Docker Hub Access Token (Recommended)

1. Log in to [Docker Hub](https://hub.docker.com/)
2. Go to Account Settings → Security → Access Tokens
3. Click "New Access Token"
4. Give it a descriptive name (e.g., "GitHub Actions - nginx-cloudflare-jwt")
5. Select appropriate permissions (Read, Write, Delete)
6. Copy the generated token and use it as `DOCKER_HUB_PASSWORD`

## Running the Docker Workflow

The Docker workflow is **manual** and must be triggered manually:

1. Go to your repository on GitHub
2. Click **Actions** tab
3. Select **"Build and Push Docker Image"** workflow
4. Click **"Run workflow"** button
5. Enter the desired version tag (e.g., `v1.0.0`, `latest`, `beta`)
6. Click **"Run workflow"** to start the build

## Docker Image Tags

The workflow will create tags based on your input:

- `hyperxpro/nginx-cloudflare-access-jwt-validator:latest` - When you input "latest"
- `hyperxpro/nginx-cloudflare-access-jwt-validator:v1.0.0` - When you input "v1.0.0"
- `hyperxpro/nginx-cloudflare-access-jwt-validator:beta` - When you input "beta"

## Usage

### Running Locally
```bash
# Pull the latest image
docker pull hyperxpro/nginx-cloudflare-access-jwt-validator:latest

# Run the container
docker run -p 8080:8080 \
  -e RUST_LOG=info \
  hyperxpro/nginx-cloudflare-access-jwt-validator:latest
```

## Workflow Features

- **Manual Control**: Docker builds are triggered manually with custom version tags
- **Caching**: Uses GitHub Actions cache for Cargo dependencies and Docker layers
- **Security**: Runs security audits and generates build attestations
- **Multi-platform**: Builds for both AMD64 and ARM64 architectures
- **Flexible Tagging**: Use any version tag you want (latest, v1.0.0, beta, etc.)
- **Automated Updates**: Dependabot keeps dependencies and actions up to date weekly

## Dependabot Configuration

Dependabot will automatically:
- **Check weekly** (every Monday at 9:00 AM) for updates
- **Group related updates** into single PRs for easier review
- **Limit to 5 open PRs** maximum to avoid spam
- **Update GitHub Actions** to latest versions
- **Update Rust dependencies** in Cargo.toml
- **Use semantic commit messages** with appropriate prefixes

### Dependabot PR Examples

You'll receive PRs like:
- `ci: bump actions/checkout from v4 to v5` (GitHub Actions updates)
- `deps: bump axum from 0.8.4 to 0.8.5` (Rust dependency updates)

All Dependabot PRs will automatically trigger the CI workflow to ensure updates don't break anything.

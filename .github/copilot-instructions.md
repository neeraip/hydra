# Copilot Instructions

## PR Titles

PR titles must follow the Conventional Commits format:

```
<type>(<optional scope>): <description>
```

Valid types: `feat`, `fix`, `chore`, `docs`, `style`, `refactor`, `test`, `ci`, `perf`, `build`, `revert`

Examples:
- `feat(engine): add FAVAD leakage model`
- `fix(solver): correct head-loss exponent for Chezy-Manning`
- `docs: update INP format support table`
- `chore: bump version to 1.2.0`
- `refactor(io): split inp_reader into section parsers`
- `test(hydraulics): add regression fixture for KY10`

---
description: Add pull requests to the Drasi Project Board and set their status to Doing
on:
  pull_request:
    types: [opened, reopened, ready_for_review]
  workflow_dispatch:
permissions:
  contents: read
  pull-requests: read
  issues: read
tools:
  github:
    toolsets: [default, projects]
    github-token: ${{ secrets.PROJECTS_PAT }}
safe-outputs:
  update-project:
    project: https://github.com/orgs/drasi-project/projects/<DRASI_PROJECT_BOARD_ID>
    max: 5
    github-token: ${{ secrets.PROJECTS_PAT }}
timeout-minutes: 5
env:
  DRASI_PROJECT_BOARD_ID: ${{ vars.DRASI_PROJECT_BOARD_ID }}
  DRASI_PROJECT_BOARD_URL: https://github.com/orgs/drasi-project/projects/${{ vars.DRASI_PROJECT_BOARD_ID }}
---

# Add pull requests to Drasi Project Board

You are an automation agent that ensures every pull request is tracked on the Drasi Project Board with `Status: Doing`.

## What to do

1. **Get PR details** from the event payload (number, title, URL).
2. **Validate configuration**:
   - Confirm `DRASI_PROJECT_BOARD_ID` and `DRASI_PROJECT_BOARD_URL` are non-empty. If missing, fail with a clear message so maintainers can set the repository variable.
   - Use the `Status` field value `Doing`.
3. **Add or update the project item** using the `update-project` tool:
   ```json
   {
     "type": "update_project",
     "project": "<DRASI_PROJECT_BOARD_URL>",
     "content_type": "pull_request",
     "content_number": <pr_number>,
     "fields": { "Status": "Doing" }
   }
   ```
   - If the PR is already on the board, ensure its `Status` is set to `Doing`.
4. **Report** a concise outcome:
   - Success message with project URL.
   - If the board or Status field is unreachable, fail with actionable guidance.

## Notes

- Use the provided `PROJECTS_PAT` token (projects:write scope) for project updates.
- Do not modify PR labels, reviewers, or milestones.
- Keep output short and actionable.

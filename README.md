# Project Risk Intelligence

Project Risk Intelligence is a desktop-first PMO workspace for monitoring project portfolios, surfacing milestone risk, and preparing focused local AI insights. This repository contains the Stage 1–4 foundation: the application shell, dashboard prototype, local SQLite data layer, and safe Excel import workflow.

## Technology stack

- React 18, TypeScript, Vite, and Tailwind CSS
- React Router and TanStack Query
- Lucide React and Recharts
- Tauri 2 and Rust

## Install

Requirements: Node.js 20+, Rust stable, and the platform prerequisites listed in the Tauri documentation.

```bash
npm install
```

## Development

Run the browser-based React development preview:

```bash
npm run dev
```

Run the full desktop application:

```bash
npm run tauri dev
```

## Build

Build the frontend:

```bash
npm run build
```

Create a desktop application bundle:

```bash
npm run tauri build
```

## Current scope

Implemented in this stage:

- Fixed sidebar and responsive desktop application shell
- Portfolio dashboard with four KPI cards
- Risk-prioritized project portfolio table
- Rule-based AI summary presentation
- Upcoming milestone presentation
- Routed placeholders for Projects, Risks, AI Insights, Data Sync, and Settings
- Centralized typed demo data in `src/data/demoData.ts`
- SQLite database initialization in the Tauri application data directory
- Transactional, versioned schema migrations
- Tables for projects, milestones, risks, insights, import history, and settings
- Foreign keys, integrity constraints, and query indexes
- Rust repository modules and typed Tauri database commands
- TypeScript database client boundary for future TanStack Query integration
- Native `.xlsx` file selection from the Data Sync page
- Full workbook, sheet, column, project ID, duplicate ID, and date validation
- Excel serial date conversion to `YYYY-MM-DD`
- Full dataset replacement inside a single SQLite transaction
- Import history with successful and failed attempts

Not implemented yet:

- Deterministic milestone and risk engines
- Project search, filters, sorting, and pagination
- Project detail, current-state risk charts, and local Q&A
- External AI or enterprise LLM integration

## Local database

On first desktop launch, Tauri creates `project-risk.db` inside the operating system's application data directory for `com.projectrisk.intelligence`. Migration version 1 is applied atomically and enables foreign keys, WAL journaling, and a five-second busy timeout.

The React application accesses the database only through `src/services/database/databaseClient.ts`. UI components do not contain SQL or open the database directly.

## Excel import safety

The importer validates the workbook and `Active_Project_List` sheet completely before beginning a SQLite transaction. Only after parsing and validation succeed does it delete and replace business data, insert normalized projects and milestones, and commit. Any failure rolls back and preserves the previous dataset.

## Next phase: Projects

The next phase should connect the Projects page to paginated SQLite queries, then add search, filters, sorting, and project detail navigation without fetching the entire dataset into React.

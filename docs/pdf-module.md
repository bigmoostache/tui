# PDF Module Design — `cp-mod-typst`

## Status: Design Phase (2026-02-21)

---

## Core Decisions (Settled)

### Architecture
- **Lightweight module** (`cp-mod-typst` crate) — minimal tools, no custom panel
- **Dependencies**: core + callback + tree modules
- Relies on **existing tools** for editing (standard `Edit`/`Open` on `.typ` files)
- Relies on **existing callback system** for auto-compile on `.typ` edits
- Relies on **existing tree panel** for displaying document status

### Typst Compiler
- **Embedded** — typst crate compiled into Context Pilot binary (direct dependency)
- No external `typst` CLI dependency required
- User never needs to install typst separately

### File Layout
```
.context-pilot/pdf/
├── templates/          # .typ template files for uniform styling
│   ├── invoice.typ
│   ├── report.typ
│   └── letterhead.typ
├── documents/          # .typ source files (the actual docs)
│   ├── q1-report.typ
│   └── invoice-042.typ
└── output/             # compiled PDFs (intermediate, gitignored)
    ├── q1-report.pdf
    └── invoice-042.pdf
```

### Config — Global (`.context-pilot/config.json`)
- **Document map**: each document has a source `.typ` path and a target destination for the final PDF
- Example:
```json
{
  "typst": {
    "documents": {
      "q1-report": {
        "source": ".context-pilot/pdf/documents/q1-report.typ",
        "target": "./reports/q1-report.pdf"
      },
      "invoice-042": {
        "source": ".context-pilot/pdf/documents/invoice-042.typ",
        "target": "./invoices/invoice-042.pdf"
      }
    }
  }
}
```

### Config — Worker State (per-worker)
- Which documents are currently open (loaded via standard `Open` tool)
- Compile status per document (last compile time, success/error)

### Templates
- `.typ` files in `.context-pilot/pdf/templates/`
- Provide uniform styling across the project (fonts, colors, headers, footers, page layout)
- Documents `#import` them: `#import "../templates/report.typ": *`
- LLM can read, create, and edit templates using standard `Open`/`Edit` tools

---

## Tools (Minimal — Lean on Existing)

### `pdf_create`
- Creates a new `.typ` document in `.context-pilot/pdf/documents/`
- Parameters: `name` (document name), `target` (required — destination path for compiled PDF), `template` (optional — which template to base it on)
- Scaffolds a default `.typ` file (with template import if specified)
- Registers the document in `config.json` document map
- **Automatically opens** the new `.typ` file (via standard `Open`) so the LLM can immediately edit it

### `pdf_edit`
- **Metadata only** — updates document config (target path, template association)
- Does NOT edit `.typ` content — use the standard `Edit` tool for that
- Parameters: `name` (document name), `target` (new target path), `delete` (bool — removes source, output, target PDF, and config entry)
- Supports `delete: true` to cleanly remove a document (same pattern as `todo_update`)

### No `pdf_compile`, `pdf_list`, `pdf_open_editor`, or custom panel
- Compilation is automatic (callback-driven on `.typ` edits)
- Document listing is shown in the **tree panel** with smart annotations
- Editing is done with standard `Open` + `Edit` tools

---

## Module Activation

### Lazy Setup
- On activation: creates `.context-pilot/pdf/` folder structure only (documents/, templates/, output/)
- **Templates**: built-in starters copied on first `pdf_create` call (not on activation)
- **Callback**: `typst-compile` callback registered on first `pdf_create` call
- This keeps activation lightweight — no side effects until the user actually creates a document

---

## Tree Integration

### Auto-Annotate Targets
- Module hooks into tree rendering to annotate PDF target files
- When a file in the project matches a document's target path, the tree shows its `.typ` source:
  ```
  ├── reports/
  │   └── q1-report.pdf          # → .context-pilot/pdf/documents/q1-report.typ
  ```
- This is automatic — no manual `tree_describe` needed
- The `.context-pilot/pdf/` subtree shows compile status (✓/✗) on each document

---

## No Custom Panel — Tree Integration Instead

### Tree Panel Annotations
- When a PDF document exists in the config, the **tree panel** shows the target file with a description pointing back to its `.typ` source:
  ```
  ├── reports/
  │   └── q1-report.pdf          # → edit: .context-pilot/pdf/documents/q1-report.typ
  ```
- The `.context-pilot/pdf/` folder in the tree shows compile status indicators (✓/✗) on each document

### Why No Panel?
- The tree panel already shows file status
- The standard `Open` tool already loads files into context
- The standard `Edit` tool already edits files
- Adding a custom panel would duplicate functionality that already exists

---

## Auto-Compile (Callback Integration)

### How It Works
- Module uses the **existing callback system** as-is — no special hooks or internal overrides
- On module activation, registers a standard callback via `Callback_upsert`:
  - **Name**: `typst-compile`
  - **Pattern**: `.context-pilot/pdf/**/*.typ`
  - **Blocking**: yes
  - **Script**: runs embedded typst compiler to validate → compile → copy to target
- The callback appears in the Callbacks panel like any user callback
- User can toggle it, inspect its script, etc. — full transparency

### Pipeline (on each .typ edit)
  1. **Validate** — check typst syntax via embedded compiler
  2. **Compile** — generate PDF to `.context-pilot/pdf/output/`
  3. **Copy** — copy PDF to target destination (from config.json document map)
- Errors surface in the `Edit` tool result (same pattern as `rust-check` callback)

### Template Edits
- Editing a template triggers recompile of **all documents that import it**
- The callback detects whether the edited file is a template and finds dependent documents

---

## Workflow Example

```
User: "Create a quarterly report PDF"

1. LLM calls pdf_create(name: "q1-report", target: "./reports/q1-report.pdf", template: "report")
   → Creates .context-pilot/pdf/documents/q1-report.typ (with #import "../templates/report.typ": *)
   → Registers in config.json
   → Auto-opens the .typ file

2. LLM calls Edit(file_path: ".context-pilot/pdf/documents/q1-report.typ", ...)
   → Writes the document content
   → Callback fires: validate → compile → copy to ./reports/q1-report.pdf
   → Edit tool result includes: "✓ PDF compiled → ./reports/q1-report.pdf"

3. User: "Change the heading color"
   → LLM opens and edits the template .typ file
   → Callback fires: recompiles all dependent documents
```

---

## Open Questions

<!-- Answers will be appended as design progresses -->

## Deferred

### Custom Fonts
- Not in scope for v1
- Future: `.context-pilot/pdf/fonts/` folder for project-specific fonts

---

## Built-in Templates

### Shipped Starters
- Module ships with 2-3 built-in `.typ` templates:
  - **report.typ** — professional report layout (title page, headers, footers, page numbers)
  - **invoice.typ** — invoice/billing template (logo placeholder, line items table, totals)
  - **letter.typ** — formal letter (letterhead, date, address block, signature)
- On module activation (or first `pdf_create`), copies to `.context-pilot/pdf/templates/` if not already present
- User/LLM can modify or create new templates freely

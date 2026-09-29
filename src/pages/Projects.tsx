import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import {
  ArrowDown,
  ArrowUp,
  ChevronLeft,
  ChevronRight,
  Filter,
  FolderSearch2,
  Search,
  X
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { PageHeader } from "../components/common/PageHeader";
import { RiskBadge, StatusBadge } from "../components/common/Badge";
import { getProjects } from "../services/database/databaseClient";
import type { ProjectFilterOptions, ProjectListItem } from "../types/projects";

const desktopMode = isTauri();
const emptyFilters: ProjectFilterOptions = {
  customers: [], projectManagers: [], acqManagers: [], engineSegments: [],
  productCategories: [], emissions: [], projectTypes: [], systems: [],
  projectStatuses: [], riskLevels: []
};
const initialSelections = {
  customer: "", projectManager: "", acqManager: "", engineSegment: "",
  productCategory: "", emission: "", projectType: "", system: "",
  projectStatus: "", riskLevel: ""
};
type Selections = typeof initialSelections;

export function Projects() {
  const navigate = useNavigate();
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [selections, setSelections] = useState<Selections>(initialSelections);
  const [filtersOpen, setFiltersOpen] = useState(true);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [sortBy, setSortBy] = useState("riskScore");
  const [sortDirection, setSortDirection] = useState<"asc" | "desc">("desc");

  useEffect(() => {
    const timer = window.setTimeout(() => setDebouncedSearch(search.trim()), 250);
    return () => window.clearTimeout(timer);
  }, [search]);

  useEffect(() => setPage(1), [debouncedSearch, selections, pageSize, sortBy, sortDirection]);

  const query = useQuery({
    queryKey: ["projects", debouncedSearch, selections, page, pageSize, sortBy, sortDirection],
    queryFn: () => getProjects({
      search: debouncedSearch,
      ...selections,
      sortBy,
      sortDirection,
      limit: pageSize,
      offset: (page - 1) * pageSize
    }),
    enabled: desktopMode,
    placeholderData: (previous) => previous
  });

  const data = query.data;
  const options = data?.filters ?? emptyFilters;
  const totalPages = Math.max(1, Math.ceil((data?.total ?? 0) / pageSize));
  const activeFilters = useMemo(
    () => Object.values(selections).filter(Boolean).length,
    [selections]
  );

  function updateFilter(key: keyof Selections, value: string) {
    setSelections((current) => ({ ...current, [key]: value }));
  }

  function changeSort(column: string) {
    if (sortBy === column) setSortDirection((current) => current === "asc" ? "desc" : "asc");
    else {
      setSortBy(column);
      setSortDirection("asc");
    }
  }

  function openProject(project: ProjectListItem) {
    navigate(`/projects/${project.id}`);
  }

  return (
    <div className={`page projects-page${filtersOpen ? " filters-open" : ""}`}>
      <PageHeader
        eyebrow="Portfolio Directory"
        title="Projects"
        description="Browse and analyze all active projects."
        aside={`${data?.total ?? 0} projects in current view`}
      />

      <section className="projects-workbench card">
        <div className="projects-toolbar">
          <label className="project-search">
            <Search size={18} />
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search name, MCR-ID, customer, P-ID, manager or system…"
              aria-label="Search projects"
            />
            {search && <button type="button" onClick={() => setSearch("")} aria-label="Clear search"><X size={16} /></button>}
          </label>
          <button type="button" className={`filter-toggle ${filtersOpen ? "active" : ""}`} onClick={() => setFiltersOpen((open) => !open)}>
            <Filter size={17} /> Filters {activeFilters > 0 && <span>{activeFilters}</span>}
          </button>
          {(activeFilters > 0 || search) && (
            <button type="button" className="clear-filters" onClick={() => { setSelections(initialSelections); setSearch(""); }}>
              Clear filters
            </button>
          )}
        </div>

        {filtersOpen && (
          <div className="project-filters">
            <FilterSelect label="Customer" value={selections.customer} options={options.customers} onChange={(value) => updateFilter("customer", value)} />
            <FilterSelect label="Project Manager" value={selections.projectManager} options={options.projectManagers} onChange={(value) => updateFilter("projectManager", value)} />
            <FilterSelect label="Acq Manager" value={selections.acqManager} options={options.acqManagers} onChange={(value) => updateFilter("acqManager", value)} />
            <FilterSelect label="Engine Segment" value={selections.engineSegment} options={options.engineSegments} onChange={(value) => updateFilter("engineSegment", value)} />
            <FilterSelect label="Product Category" value={selections.productCategory} options={options.productCategories} onChange={(value) => updateFilter("productCategory", value)} />
            <FilterSelect label="Emission" value={selections.emission} options={options.emissions} onChange={(value) => updateFilter("emission", value)} />
            <FilterSelect label="A/K Project" value={selections.projectType} options={options.projectTypes} onChange={(value) => updateFilter("projectType", value)} />
            <FilterSelect label="System" value={selections.system} options={options.systems} onChange={(value) => updateFilter("system", value)} />
            <FilterSelect label="PRJ Status" value={selections.projectStatus} options={options.projectStatuses} onChange={(value) => updateFilter("projectStatus", value)} />
            <FilterSelect label="Risk Level" value={selections.riskLevel} options={options.riskLevels} onChange={(value) => updateFilter("riskLevel", value)} />
          </div>
        )}

        {!desktopMode ? (
          <div className="projects-empty"><FolderSearch2 size={30} /><strong>Desktop application required</strong><p>Project records are stored in the local Tauri SQLite database.</p></div>
        ) : query.isError ? (
          <div className="projects-empty error"><FolderSearch2 size={30} /><strong>Projects could not be loaded</strong><p>{String(query.error)}</p></div>
        ) : (
          <>
            <div className="projects-table-wrap" role="region" aria-label="Projects table" tabIndex={0}>
              <table className="projects-table">
                <thead><tr>
                  <SortableHeader label="MCR-ID" column="mcrId" current={sortBy} direction={sortDirection} onSort={changeSort} />
                  <SortableHeader label="Project Name" column="projectName" current={sortBy} direction={sortDirection} onSort={changeSort} />
                  <SortableHeader label="Customer" column="customer" current={sortBy} direction={sortDirection} onSort={changeSort} />
                  <SortableHeader label="Project Manager" column="projectManager" current={sortBy} direction={sortDirection} onSort={changeSort} />
                  <SortableHeader label="System" column="system" current={sortBy} direction={sortDirection} onSort={changeSort} />
                  <SortableHeader label="Product Category" column="productCategory" current={sortBy} direction={sortDirection} onSort={changeSort} />
                  <SortableHeader label="PRJ Status" column="projectStatus" current={sortBy} direction={sortDirection} onSort={changeSort} />
                  <SortableHeader label="Risk" column="riskScore" current={sortBy} direction={sortDirection} onSort={changeSort} />
                  <SortableHeader label="Next Milestone" column="nextMilestone" current={sortBy} direction={sortDirection} onSort={changeSort} />
                </tr></thead>
                <tbody>
                  {data?.items.map((project) => (
                    <tr key={project.id} tabIndex={0} onClick={() => openProject(project)} onKeyDown={(event) => event.key === "Enter" && openProject(project)}>
                      <td><span className="mcr-code">{project.mcrId}</span></td>
                      <td><strong>{project.projectName}</strong></td>
                      <td>{project.customer}</td>
                      <td>{project.projectManager}</td>
                      <td>{project.system}</td>
                      <td>{project.productCategory}</td>
                      <td><StatusBadge status={project.projectStatus} /></td>
                      <td><div className="risk-cell"><RiskBadge level={project.riskLevel} /><span>{project.riskScore}</span></div></td>
                      <td><strong className="milestone-name">{project.nextMilestone}</strong><small>{formatDate(project.nextMilestoneDate)}</small></td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {!query.isLoading && data?.items.length === 0 && <div className="projects-empty"><FolderSearch2 size={30} /><strong>No matching projects</strong><p>Adjust the search or clear one or more filters.</p></div>}
              {query.isLoading && <div className="projects-loading">Loading project portfolio…</div>}
            </div>

            <div className="projects-pagination">
              <span>{data?.total ? `${(page - 1) * pageSize + 1}–${Math.min(page * pageSize, data.total)} of ${data.total}` : "0 projects"}</span>
              <label>Rows <select value={pageSize} onChange={(event) => setPageSize(Number(event.target.value))}><option>20</option><option>50</option><option>100</option></select></label>
              <div>
                <button type="button" onClick={() => setPage((current) => Math.max(1, current - 1))} disabled={page === 1}><ChevronLeft size={17} /></button>
                <span>Page {page} of {totalPages}</span>
                <button type="button" onClick={() => setPage((current) => Math.min(totalPages, current + 1))} disabled={page >= totalPages}><ChevronRight size={17} /></button>
              </div>
            </div>
          </>
        )}
      </section>
    </div>
  );
}

function FilterSelect({ label, value, options, onChange }: { label: string; value: string; options: string[]; onChange: (value: string) => void }) {
  return <label><span>{label}</span><select value={value} onChange={(event) => onChange(event.target.value)}><option value="">All</option>{options.map((option) => <option key={option}>{option}</option>)}</select></label>;
}

function SortableHeader({ label, column, current, direction, onSort }: { label: string; column: string; current: string; direction: "asc" | "desc"; onSort: (column: string) => void }) {
  const active = current === column;
  return <th><button type="button" onClick={() => onSort(column)}>{label}{active && (direction === "asc" ? <ArrowUp size={13} /> : <ArrowDown size={13} />)}</button></th>;
}

function formatDate(value: string | null): string {
  if (!value) return "—";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(`${value}T00:00:00`));
}

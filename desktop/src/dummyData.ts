// Fixture data used to fill Meridian screens that aren't backed by the live
// snapshot yet (workers, workflow definitions, sparkline series). These are
// presentation-only — replace with real backend feeds when available.

export interface DummyWorker {
  id: string;
  host: string;
  region: "sf" | "sea" | "nyc";
  cores: number;
  ram: string;
  status: "online" | "idle" | "offline";
  load: number;
}

export interface DummyWorkflow {
  id: string;
  name: string;
  description: string;
  steps: string[];
  timeoutMin: number;
  uses: number;
}

export const DUMMY_WORKERS: DummyWorker[] = [
  { id: "w-aurora",   host: "aurora.sf.openai.internal",   region: "sf",  cores: 32, ram: "128 GB", status: "online",  load: 0.62 },
  { id: "w-borealis", host: "borealis.sf.openai.internal", region: "sf",  cores: 32, ram: "128 GB", status: "online",  load: 0.41 },
  { id: "w-cygnus",   host: "cygnus.sea.openai.internal",  region: "sea", cores: 64, ram: "256 GB", status: "online",  load: 0.88 },
  { id: "w-draco",    host: "draco.sea.openai.internal",   region: "sea", cores: 64, ram: "256 GB", status: "online",  load: 0.23 },
  { id: "w-eridanus", host: "eridanus.nyc.openai.internal",region: "nyc", cores: 16, ram: "64 GB",  status: "idle",    load: 0.04 },
  { id: "w-fornax",   host: "fornax.nyc.openai.internal",  region: "nyc", cores: 16, ram: "64 GB",  status: "offline", load: 0.0  },
];

export const DUMMY_WORKFLOWS: DummyWorkflow[] = [
  { id: "wf-bugfix",   name: "Bugfix",   description: "Reproduce, write failing test, fix, verify.",  steps: ["reproduce", "write-failing-test", "patch", "verify", "open-pr"], timeoutMin: 45,  uses: 142 },
  { id: "wf-feature",  name: "Feature",  description: "Design doc → implementation → tests → PR.",    steps: ["design", "scaffold", "implement", "test", "open-pr"],          timeoutMin: 120, uses: 87  },
  { id: "wf-refactor", name: "Refactor", description: "No behavior change. Diff stays small.",        steps: ["plan", "apply", "verify-no-regression", "open-pr"],            timeoutMin: 60,  uses: 41  },
  { id: "wf-docs",     name: "Docs",     description: "Update inline docs and README from code.",     steps: ["scan", "draft", "lint", "open-pr"],                            timeoutMin: 20,  uses: 213 },
  { id: "wf-triage",   name: "Triage",   description: "Read issue, label, suggest workflow.",         steps: ["read", "classify", "comment"],                                 timeoutMin: 5,   uses: 506 },
];

export const DUMMY_RUNS_PER_HOUR = [3, 2, 1, 1, 0, 1, 2, 4, 7, 11, 14, 16, 18, 15, 19, 22, 17, 14, 12, 10, 8, 6, 4, 3];

export const DUMMY_DASHBOARD = {
  merged24h: 14,
  cost24h: 142.88,
  successRate: 0.847,
  runsPerHour: DUMMY_RUNS_PER_HOUR,
};

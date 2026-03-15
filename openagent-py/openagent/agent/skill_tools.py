"""skill_tools — discover and run skills as non-blocking background jobs.

Skills are Python (.py) or shell (.sh) scripts in the skills/ directory.
Each skill declares metadata in its header so the agent can discover and
trigger it by name or by matching a user request to known triggers.

Skill metadata format
---------------------
Python scripts — in the module docstring (first string literal):

    skill: mailgov-download-inbox
    description: Download all inbox emails as .eml files from mail.gov.in
    triggers: download emails, sync inbox, download mail, backup email
    requires: MAILGOV_STATE
    optional: MAILGOV_OUT, MAILGOV_LIMIT
    category: email

Bash scripts — in leading comments:

    # skill: my-skill
    # description: What it does
    # triggers: phrase one, phrase two
    # requires: ENV_VAR
    # optional: ENV_VAR2
    # category: category

Four native tools are registered:
    skill.list    — list all discoverable skills with metadata
    skill.run     — start a skill in a background thread, returns job_id
    skill.status  — check job status + recent output lines
    skill.cancel  — kill a running job
"""

from __future__ import annotations

import asyncio
import logging
import os
import re
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

SKILLS_ROOT = Path(os.environ.get("SKILLS_DIR", "skills"))
OUTPUT_TAIL = 50   # lines of output kept per job


# ---------------------------------------------------------------------------
# Metadata
# ---------------------------------------------------------------------------

@dataclass
class SkillMeta:
    name: str
    description: str
    path: str
    triggers: list[str] = field(default_factory=list)
    requires: list[str] = field(default_factory=list)
    optional: list[str] = field(default_factory=list)
    category: str = "general"

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "path": self.path,
            "triggers": self.triggers,
            "requires": self.requires,
            "optional": self.optional,
            "category": self.category,
        }


def _parse_meta(path: Path) -> SkillMeta | None:
    """Extract skill metadata from the first ~40 lines of a .py or .sh file."""
    try:
        lines = path.read_text(errors="replace").splitlines()[:40]
    except OSError:
        return None

    fields: dict[str, str] = {}

    if path.suffix == ".py":
        # Look inside the first docstring block
        in_doc = False
        for line in lines:
            stripped = line.strip()
            if not in_doc:
                if stripped.startswith('"""') or stripped.startswith("'''"):
                    in_doc = True
                    stripped = stripped.lstrip('"\'').strip()
                    if stripped:
                        _try_field(stripped, fields)
                continue
            if stripped.startswith('"""') or stripped.startswith("'''"):
                break
            _try_field(stripped, fields)
    else:
        # Bash: look for # key: value comments
        for line in lines:
            stripped = line.strip().lstrip("#").strip()
            _try_field(stripped, fields)

    if "skill" not in fields:
        return None

    def split_csv(s: str) -> list[str]:
        return [x.strip() for x in s.split(",") if x.strip()]

    return SkillMeta(
        name=fields["skill"],
        description=fields.get("description", ""),
        path=str(path),
        triggers=split_csv(fields.get("triggers", "")),
        requires=split_csv(fields.get("requires", "")),
        optional=split_csv(fields.get("optional", "")),
        category=fields.get("category", "general"),
    )


def _try_field(line: str, fields: dict[str, str]) -> None:
    m = re.match(r"^(skill|description|triggers|requires|optional|category)\s*:\s*(.+)$", line)
    if m:
        fields[m.group(1)] = m.group(2).strip()


def discover_skills() -> list[SkillMeta]:
    """Scan SKILLS_ROOT recursively for .py and .sh files with skill metadata."""
    if not SKILLS_ROOT.is_dir():
        return []
    skills = []
    for p in sorted(SKILLS_ROOT.rglob("*")):
        if p.suffix in (".py", ".sh") and p.is_file():
            meta = _parse_meta(p)
            if meta:
                skills.append(meta)
    return skills


# ---------------------------------------------------------------------------
# Job registry
# ---------------------------------------------------------------------------

@dataclass
class SkillJob:
    job_id: str
    skill_name: str
    status: str           # running | done | failed | cancelled
    output_lines: list[str] = field(default_factory=list)
    returncode: int | None = None
    _proc: asyncio.subprocess.Process | None = field(default=None, repr=False)

    def tail(self, n: int = OUTPUT_TAIL) -> str:
        return "\n".join(self.output_lines[-n:])


_jobs: dict[str, SkillJob] = {}


# ---------------------------------------------------------------------------
# Execution
# ---------------------------------------------------------------------------

async def _drain(stream: asyncio.StreamReader, job: SkillJob) -> None:
    while True:
        line = await stream.readline()
        if not line:
            break
        decoded = line.decode(errors="replace").rstrip()
        job.output_lines.append(decoded)
        # keep ring-buffer bounded
        if len(job.output_lines) > OUTPUT_TAIL * 4:
            job.output_lines = job.output_lines[-OUTPUT_TAIL * 2:]


async def _run_skill(job: SkillJob, meta: SkillMeta, env: dict[str, str]) -> None:
    merged_env = {**os.environ, **env}
    path = Path(meta.path)
    if path.suffix == ".py":
        cmd = ["python3", str(path)]
    else:
        cmd = ["bash", str(path)]

    try:
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT,
            env=merged_env,
        )
        job._proc = proc
        await _drain(proc.stdout, job)  # type: ignore[arg-type]
        await proc.wait()
        job.returncode = proc.returncode
        job.status = "done" if proc.returncode == 0 else "failed"
        logger.info("skill %s finished: job=%s rc=%s", meta.name, job.job_id, proc.returncode)
    except Exception as exc:
        job.output_lines.append(f"ERROR: {exc}")
        job.status = "failed"
        logger.exception("skill %s job %s crashed", meta.name, job.job_id)


# ---------------------------------------------------------------------------
# Native tool handlers
# ---------------------------------------------------------------------------

async def _skill_list(_session_key: str, _args: dict[str, Any]) -> str:
    skills = discover_skills()
    if not skills:
        return "No skills found. Skills are .py or .sh files in skills/ with a skill metadata header."
    lines = [f"Found {len(skills)} skill(s):\n"]
    for s in skills:
        lines.append(f"  name:        {s.name}")
        lines.append(f"  description: {s.description}")
        lines.append(f"  category:    {s.category}")
        if s.triggers:
            lines.append(f"  triggers:    {', '.join(s.triggers)}")
        if s.requires:
            lines.append(f"  requires:    {', '.join(s.requires)}")
        if s.optional:
            lines.append(f"  optional:    {', '.join(s.optional)}")
        lines.append(f"  path:        {s.path}")
        lines.append("")
    return "\n".join(lines)


async def _skill_run(_session_key: str, args: dict[str, Any]) -> str:
    name = args.get("name", "").strip()
    env_overrides: dict[str, str] = {
        str(k): str(v) for k, v in (args.get("env") or {}).items()
    }

    skills = discover_skills()
    meta = next((s for s in skills if s.name == name), None)
    if meta is None:
        available = ", ".join(s.name for s in skills) or "none"
        return f"Skill '{name}' not found. Available: {available}"

    # Check required env vars are present
    missing = [r for r in meta.requires if r not in env_overrides and r not in os.environ]
    if missing:
        return (
            f"Skill '{name}' requires env vars that are not set: {', '.join(missing)}. "
            f"Pass them via the env parameter: {{\"env\": {{\"{missing[0]}\": \"value\"}}}}"
        )

    job_id = uuid.uuid4().hex[:12]
    job = SkillJob(job_id=job_id, skill_name=name, status="running")
    _jobs[job_id] = job

    # Fire and forget — non-blocking
    asyncio.create_task(_run_skill(job, meta, env_overrides))

    logger.info("skill %s started: job=%s", name, job_id)
    return (
        f"Skill '{name}' started in background.\n"
        f"job_id: {job_id}\n"
        f"Use skill.status with job_id='{job_id}' to check progress."
    )


async def _skill_status(_session_key: str, args: dict[str, Any]) -> str:
    job_id = args.get("job_id", "").strip()
    job = _jobs.get(job_id)
    if job is None:
        active = [f"{j.job_id} ({j.skill_name}, {j.status})" for j in _jobs.values()]
        return f"Job '{job_id}' not found. Active jobs: {', '.join(active) or 'none'}"

    n = int(args.get("lines", OUTPUT_TAIL))
    tail = job.tail(n)
    rc = f", returncode={job.returncode}" if job.returncode is not None else ""
    header = f"Job {job_id} | skill={job.skill_name} | status={job.status}{rc}\n"
    return header + (tail if tail else "(no output yet)")


async def _skill_cancel(_session_key: str, args: dict[str, Any]) -> str:
    job_id = args.get("job_id", "").strip()
    job = _jobs.get(job_id)
    if job is None:
        return f"Job '{job_id}' not found."
    if job.status != "running":
        return f"Job '{job_id}' is not running (status={job.status})."
    if job._proc is not None:
        try:
            job._proc.terminate()
        except ProcessLookupError:
            pass
    job.status = "cancelled"
    return f"Job '{job_id}' (skill={job.skill_name}) cancelled."


# ---------------------------------------------------------------------------
# Registration helper
# ---------------------------------------------------------------------------

def make_skill_tools() -> list[tuple[str, str, dict[str, Any], Any]]:
    """Return (name, description, params_schema, handler) tuples for ToolRegistry."""
    return [
        (
            "skill.list",
            "List all available skills with their names, descriptions, trigger phrases, "
            "and required env vars. Call this to discover what skills exist before running one.",
            {"type": "object", "properties": {}},
            _skill_list,
        ),
        (
            "skill.run",
            "Run a skill by name in a background thread (non-blocking). Returns a job_id immediately. "
            "Pass env vars the skill needs via the env dict. "
            "Use skill.status to check progress.",
            {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Skill name from skill.list"},
                    "env": {
                        "type": "object",
                        "description": "Environment variables to pass to the skill (e.g. {\"MAILGOV_STATE\": \"./data/mailgov-auth.json\"})",
                        "additionalProperties": {"type": "string"},
                    },
                },
                "required": ["name"],
            },
            _skill_run,
        ),
        (
            "skill.status",
            "Check the status and recent output of a running or completed skill job.",
            {
                "type": "object",
                "properties": {
                    "job_id": {"type": "string", "description": "Job ID returned by skill.run"},
                    "lines": {"type": "number", "description": f"Number of output lines to return (default {OUTPUT_TAIL})"},
                },
                "required": ["job_id"],
            },
            _skill_status,
        ),
        (
            "skill.cancel",
            "Cancel a running skill job.",
            {
                "type": "object",
                "properties": {
                    "job_id": {"type": "string", "description": "Job ID returned by skill.run"},
                },
                "required": ["job_id"],
            },
            _skill_cancel,
        ),
    ]

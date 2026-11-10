#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Checks the knowledge bundle (knowledge/) against OKF v0.2 and the osage
engineering profile (knowledge/profile.md).

    python3 tools/check-knowledge.py [BUNDLE]

It checks, for every card: the front matter and its required fields, the
sections its type asks for and their order, that what it cites is declared,
that its links lead somewhere inside the bundle, and that every address into
this repository names a file that exists. It checks that the index lists
every card and that the log is dated. Needs PyYAML.
"""

import pathlib
import re
import sys
from datetime import date, datetime

import yaml

REPO = pathlib.Path(__file__).resolve().parent.parent
REPO_URL = "https://github.com/smichea/osage"
OKF_VERSION = "0.2"

# The profile: fields and sections per type, sections in this order.
PROFILE = {
    "Module": {
        "folder": "modules",
        "fields": ["resource", "licence"],
        "sections": ["Purpose", "Concepts", "Architecture", "Interfaces", "Invariants", "Knowledge", "Reuse", "Limits"],
    },
    "Technique": {
        "folder": "techniques",
        "fields": ["resource", "licence", "sources", "tiers"],
        "sections": ["What it does", "How", "Settings per tier", "Cost", "Limits", "Oracle", "Reuse"],
    },
    "Algorithm": {
        "folder": "algorithms",
        "fields": ["resource", "licence", "sources"],
        "sections": ["Problem", "Idea", "Method", "Invariants", "Cost", "Failure modes", "Alternatives", "Oracle", "Reuse"],
    },
    "Attested Computation": {
        "folder": "oracles",
        "fields": ["runtime", "parameters", "executor", "attester"],
        "sections": ["Computation"],
    },
    "Specification": {"folder": "", "fields": [], "sections": []},
}
EVERY_CARD = ["title", "description", "status", "generated"]
STATUSES = ["draft", "stable", "deprecated"]
TIERS = ["T1", "T2", "T3", "T4"]
RESERVED = ["index.md", "log.md"]

FENCE = re.compile(r"^(```|~~~).*?^\1[^\n]*$", re.M | re.S)
INLINE_CODE = re.compile(r"`[^`\n]*`")
LINK = re.compile(r"(?<!!)\[[^\]\n]*\]\(\s*<?([^)\s>]+)>?[^)\n]*\)")
ADDRESS = re.compile(re.escape(REPO_URL) + r"/(?:blob|tree)/[^/\s)#>\"']+/([^\s)#>\"']+)")
FOOTNOTE_USE = re.compile(r"\[\^([^\]\s]+)\]")
FOOTNOTE_TEXT = re.compile(r"^\[\^([^\]\s]+)\]:.*$", re.M)
HEADING = re.compile(r"^# +(.+?)\s*$", re.M)
FILE_NAME = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*\.md$")
ACTOR = re.compile(r"^(human:\S+|process:\S+|[^\s/:]+/\S+)$")


class Findings:
    def __init__(self):
        self.found = []

    def add(self, where, what):
        self.found.append(f"{where}: {what}")


def split(text):
    """The front matter (as YAML text) and the body of a file; None if it has no front matter."""
    if not text.startswith("---\n"):
        return None, text
    end = text.find("\n---", 3)
    if end < 0:
        return None, text
    return text[4 : end + 1], text[end + 4 :].lstrip("\n")


def prose(body):
    """The body without its code, where links and headings are examples, not claims."""
    return INLINE_CODE.sub("", FENCE.sub("", body))


def is_time(value):
    if isinstance(value, datetime):
        return True
    try:
        datetime.fromisoformat(str(value).replace("Z", "+00:00"))
        return "T" in str(value)
    except ValueError:
        return False


def check_actor_stamp(stamp, name, where, findings):
    if not isinstance(stamp, dict) or not stamp.get("by") or not stamp.get("at"):
        findings.add(where, f"`{name}` needs `by` and `at`")
        return
    if not ACTOR.match(str(stamp["by"])):
        findings.add(where, f"`{name}.by` is not an actor (`<producer>/<version>`, `human:<id>` or `process:<id>`): {stamp['by']}")
    if not is_time(stamp["at"]):
        findings.add(where, f"`{name}.at` is not an ISO 8601 date and time: {stamp['at']}")


def check_addresses(text, where, findings):
    """Every address into this repository names something that exists here."""
    for path in ADDRESS.findall(text):
        if not (REPO / path).exists():
            findings.add(where, f"no such file in the repository: {path}")


def check_links(bundle, path, body, where, findings):
    """Links to other cards lead to a file inside the bundle. Returns the files linked."""
    linked = set()
    for target in LINK.findall(prose(body)):
        if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("#"):
            continue
        file = target.split("#")[0]
        resolved = (bundle / file.lstrip("/") if file.startswith("/") else path.parent / file).resolve()
        if bundle not in resolved.parents and resolved != bundle:
            findings.add(where, f"link leaves the bundle: {target}")
        elif not resolved.exists():
            findings.add(where, f"broken link: {target}")
        else:
            linked.add(resolved)
    return linked


def check_card(bundle, path, findings):
    where = str(path.relative_to(bundle))
    text = path.read_text()
    if not FILE_NAME.match(path.name):
        findings.add(where, "file name is not lower case with hyphens")
    front_text, body = split(text)
    if front_text is None:
        findings.add(where, "no YAML front matter")
        return
    try:
        front = yaml.safe_load(front_text)
    except yaml.YAMLError as error:
        findings.add(where, f"front matter is not valid YAML: {error}")
        return
    if not isinstance(front, dict):
        findings.add(where, "front matter is not a mapping")
        return

    if "okf_version" in front:
        findings.add(where, "`okf_version` belongs to index.md only")

    kind = front.get("type")
    if not isinstance(kind, str) or not kind.strip():
        findings.add(where, "no `type`")
        return
    profile = PROFILE.get(kind)
    if profile is None:
        findings.add(where, f"type `{kind}` is not in the profile: {', '.join(PROFILE)}")
        return
    folder = str(path.parent.relative_to(bundle)).replace(".", "")
    if folder != profile["folder"]:
        findings.add(where, f"a `{kind}` card belongs in `{profile['folder'] or 'the bundle root'}`")

    for field in EVERY_CARD + profile["fields"]:
        if front.get(field) in (None, "", []) and not (field == "parameters" and front.get(field) == []):
            findings.add(where, f"no `{field}`")
    if "status" in front and front["status"] not in STATUSES:
        findings.add(where, f"`status` is {front['status']!r}, not one of {', '.join(STATUSES)}")
    if "generated" in front:
        check_actor_stamp(front["generated"], "generated", where, findings)
    for index, stamp in enumerate(front.get("verified") or []):
        check_actor_stamp(stamp, f"verified[{index}]", where, findings)
    if "stale_after" in front and not isinstance(front["stale_after"], (date, datetime)):
        findings.add(where, f"`stale_after` is not a date: {front['stale_after']}")
    if "tags" in front and not (isinstance(front["tags"], list) and all(isinstance(t, str) for t in front["tags"])):
        findings.add(where, "`tags` is not a list of words")
    for tier in front.get("tiers") or []:
        if tier not in TIERS:
            findings.add(where, f"unknown tier {tier!r}")

    resource = front.get("resource")
    if resource is not None and kind != "Attested Computation":
        if not str(resource).startswith(REPO_URL + "/"):
            findings.add(where, f"`resource` is not an address into {REPO_URL}")
    for part in ("executor", "attester"):
        if part in front and not (isinstance(front[part], dict) and front[part].get("resource")):
            findings.add(where, f"`{part}` needs a `resource`")
    if isinstance(front.get("executor"), dict) and not front["executor"].get("receipt"):
        findings.add(where, "`executor` needs a `receipt`: what a run hands to the attester")

    # What is cited is declared, and what is declared is cited.
    declared = []
    for source in front.get("sources") or []:
        if not isinstance(source, dict) or not source.get("id") or not (source.get("resource") or source.get("title")):
            findings.add(where, f"a source needs an `id` and a `resource` or a `title`: {source}")
        else:
            declared.append(str(source["id"]))
    for repeated in sorted({i for i in declared if declared.count(i) > 1}):
        findings.add(where, f"source `{repeated}` is declared twice")
    text_without_code = prose(body)
    explained = set(FOOTNOTE_TEXT.findall(text_without_code))
    used = set(FOOTNOTE_USE.findall(FOOTNOTE_TEXT.sub("", text_without_code)))
    for name in sorted(used - set(declared)):
        findings.add(where, f"cites `[^{name}]`, which is not in `sources`")
    for name in sorted(set(declared) - used):
        findings.add(where, f"source `{name}` is never cited")
    for name in sorted(set(declared) - explained):
        findings.add(where, f"source `{name}` has no footnote text (`[^{name}]: …`)")

    # The sections of the type, in the profile's order.
    headings = HEADING.findall(FENCE.sub("", body))
    wanted = profile["sections"]
    for section in wanted:
        if section not in headings:
            findings.add(where, f"no section `# {section}`")
    present = [h for h in headings if h in wanted]
    if present != [s for s in wanted if s in present]:
        findings.add(where, f"sections are out of order; the profile's order is: {', '.join(wanted)}")
    sections = re.split(r"^# +.+$", FENCE.sub(lambda m: "code", body), flags=re.M)[1:]
    for heading, content in zip(headings, sections):
        if not content.strip():
            findings.add(where, f"section `# {heading}` is empty")

    check_addresses(text, where, findings)
    check_links(bundle, path, body, where, findings)


def check_index(bundle, cards, findings):
    path = bundle / "index.md"
    if not path.exists():
        findings.add("index.md", "missing: the bundle has no entry point")
        return
    front_text, body = split(path.read_text())
    front = yaml.safe_load(front_text) if front_text else None
    if not isinstance(front, dict) or str(front.get("okf_version")) != OKF_VERSION:
        findings.add("index.md", f'front matter must declare `okf_version: "{OKF_VERSION}"`')
    elif set(front) != {"okf_version"}:
        findings.add("index.md", "front matter carries `okf_version` and nothing else")
    linked = check_links(bundle, path, body, "index.md", findings)
    check_addresses(body, "index.md", findings)
    for card in cards:
        if card.resolve() not in linked:
            findings.add("index.md", f"does not list {card.relative_to(bundle)}")


def check_log(bundle, findings):
    path = bundle / "log.md"
    if not path.exists():
        findings.add("log.md", "missing")
        return
    body = path.read_text()
    dates = re.findall(r"^## (\S+)\s*$", body, re.M)
    if not dates:
        findings.add("log.md", "no dated entry (`## YYYY-MM-DD`)")
    for entry in dates:
        try:
            date.fromisoformat(entry)
        except ValueError:
            findings.add("log.md", f"`## {entry}` is not a date")
    if dates != sorted(dates, reverse=True):
        findings.add("log.md", "entries are not newest first")
    check_links(bundle, path, body, "log.md", findings)


def check(bundle):
    findings = Findings()
    cards = sorted(p for p in bundle.rglob("*.md") if str(p.relative_to(bundle)) not in RESERVED)
    for card in cards:
        check_card(bundle, card, findings)
    check_index(bundle, cards, findings)
    check_log(bundle, findings)
    return cards, findings.found


def main():
    bundle = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else REPO / "knowledge").resolve()
    if not bundle.is_dir():
        print(f"no bundle at {bundle}")
        return 2
    cards, found = check(bundle)
    for finding in found:
        print(finding)
    print(f"{len(cards)} cards, {len(found)} findings")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())

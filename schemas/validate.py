"""Validate the example records, history events and identity against the Open Labbook schemas.

Usage:  python3 schemas/validate.py          (from the repo root)
Needs:  pip install jsonschema referencing
"""
import json, pathlib, sys
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

# Vendored into this repo: schemas/ and fixtures/model-examples/ sit
# under the repo root, which is this file's parent.
root = pathlib.Path(__file__).resolve().parent.parent
load = lambda p: json.loads((root / p).read_text())

model = load("schemas/olb-model.schema.json")
others = [load("schemas/collection-protocol.schema.json"), load("schemas/submission.schema.json")]
registry = Registry().with_resources([(s["$id"], Resource.from_contents(s)) for s in [model] + others])
for s in [model] + others:
    Draft202012Validator.check_schema(s)

def validator(ref=None):
    schema = model if ref is None else {"$ref": model["$id"] + ref}
    return Draft202012Validator(schema, registry=registry)

failures = 0
def check(v, items, label):
    global failures
    for i, item in enumerate(items):
        errs = sorted(v.iter_errors(item), key=lambda e: list(e.path))
        name = item.get("type", "") + " " + str(item.get("id", item.get("commit", i)))
        if errs:
            failures += 1
            print(f"FAIL {label} {name}")
            for e in errs[:5]:
                print("   ", list(e.absolute_path), e.message[:200])
        else:
            print(f"ok   {label} {name}")

check(validator(), load("fixtures/model-examples/records.json"), "record")
check(validator("#/$defs/historyEvent"), load("fixtures/model-examples/history.json"), "history")
check(validator("#/$defs/identity"), [load("fixtures/model-examples/identity.json")], "identity")

# Negative tests: each must FAIL validation
records = load("fixtures/model-examples/records.json")
by_type = {r["type"]: r for r in records}
neg = []
r = json.loads(json.dumps(by_type["stage_transition"])); r["fields"]["direction"] = "back"; r["fields"]["from"] = "data"; r["fields"]["to"] = "experiments"; neg.append(("back move without reason", r))
r = json.loads(json.dumps(by_type["stage_transition"])); r["fields"]["override"] = True; neg.append(("override without reason", r))
r = json.loads(json.dumps(by_type["hypothesis"])); r["fields"]["commitment"] = None; neg.append(("committed hypothesis without commitment", r))
r = json.loads(json.dumps(by_type["pledge"])); r["fields"]["kind"] = "equity"; neg.append(("pledge with financial return", r))
r = json.loads(json.dumps(by_type["pledge"])); r["fields"]["backer"] = {"private": False}; neg.append(("public pledge without backer key", r))
r = json.loads(json.dumps(by_type["review"])); r["fields"].pop("reason"); neg.append(("flag without reason", r))
r = json.loads(json.dumps(by_type["analysis"])); r["authors"][1].pop("operator"); neg.append(("agent without operator", r))
r = json.loads(json.dumps(by_type["dataset_snapshot"])); r["version"] = 2; neg.append(("snapshot edited", r))
r = json.loads(json.dumps(by_type["project"])); r["id"] = "olb:obj:wildlife"; neg.append(("project with object id", r))
v = validator()
for label, item in neg:
    if v.is_valid(item):
        failures += 1
        print(f"FAIL negative: {label} was accepted")
    else:
        print(f"ok   negative: {label} rejected")

print("\nALL PASSED" if failures == 0 else f"\n{failures} FAILURE(S)")
sys.exit(1 if failures else 0)

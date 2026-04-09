#!/usr/bin/env python3
"""Generate 100K synthetic tweets across 5 semantic clusters."""
import json, random, re, sys

TEMPLATES = {
    "supply_chain": [
        "Port congestion at {} causing {} week delays for {} shipments",
        "Semiconductor shortage hitting {} OEMs; {} fabs ramping {} nm nodes",
        "Freight rates on {} corridor up {}% YoY as {} inventories deplete",
        "Nearshoring: {} firms moving {} production from {} to {}",
    ],
    "climate": [
        "{} carbon tax at ${}/tonne squeezes {} sector margins",
        "Hurricane {} makes landfall; {} coastal infrastructure disrupted",
        "Solar capacity additions hit {} GW; {} grid operators scramble",
        "Methane satellite data reveals {} leaks exceeding {} MT/yr",
    ],
    "fintech": [
        "CBDC pilot in {} processes {} TPS with {} ms latency",
        "DeFi protocol {} exploited for ${}M; {} audit firms face scrutiny",
        "Open banking adoption in {} reaches {}% of retail accounts",
        "Stablecoin regulation: {}% reserve ratio required from Q{}",
    ],
    "health": [
        "Phase 3 trial for {} shows {}% efficacy against {}",
        "mRNA platform from {} targeting {} approved in {} months",
        "Drug shortage: {} API manufacturing in {} disrupted by {} incident",
        "Wearable biosensor tracks {} biomarkers; {} insurance partnerships",
    ],
    "geopolitics": [
        "{} imposes {} sanctions on {} exports targeting {} sector",
        "{} election: {} party wins {} seats; {} coalition forming",
        "Diplomatic expulsion: {} recalls {} ambassador over {} dispute",
        "{} military exercises near {} strait raise shipping insurance {}%",
    ],
}
FILL = ["Alpha","Beta","Nexus","28","42","Singapore","Rotterdam",
        "Germany","India","Brazil","Vietnam","12","6","18","Q3","Q4"]

def fill(t): return re.sub(r'\{\}', lambda _: random.choice(FILL), t)

n   = int(sys.argv[1]) if len(sys.argv) > 1 else 100_000
out = sys.argv[2]      if len(sys.argv) > 2 else "tweets.jsonl"
cats = list(TEMPLATES)

with open(out, "w") as f:
    for i in range(n):
        cat  = random.choice(cats)
        tmpl = random.choice(TEMPLATES[cat])
        f.write(json.dumps({"id": i, "tenant_id": i % 10,
                            "category": cat, "text": fill(tmpl)}) + "\n")
print(f"Generated {n} tweets -> {out}")

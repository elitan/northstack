import json, sys
r=json.load(open(sys.argv[1]))
print({k:r[k] for k in ['cold_start_s','image_mib']})
for k in ['fresh_idle','idle_after_seed','idle_after_load']:
    print(k, {s:(v['cpu_avg'],v['mem_avg'],v.get('mem_peak_by_container')) for s,v in r[k].items()})
by={(x['stack'],x['endpoint'],x['concurrency']):x for x in r['runs']}
print(f"{'ep':8} {'c':>3} | {'ns rps':>8} {'nx rps':>8} {'x':>5} | {'ns p50':>6} {'nx p50':>6} {'ns p99':>7} {'nx p99':>7} | {'ns cpu':>6} {'nx cpu':>6} | {'ns mem':>6} {'nx mem':>6} | err | rps/cpu ns nx")
eff=lambda y: round(y['rps']/(y['cpu_avg']/100)) if y['rps'] and y['cpu_avg'] else None
for (s,ep,c),n in by.items():
    if s!='northstack': continue
    m=by[('nextjs',ep,c)]
    print(f"{ep:8} {c:>3} | {n['rps']:>8} {m['rps']:>8} {n['rps']/m['rps']:>5.1f} | {n['p50_ms']:>6} {m['p50_ms']:>6} {n['p99_ms']:>7} {m['p99_ms']:>7} | {n['cpu_avg']:>6} {m['cpu_avg']:>6} | {n['mem_peak']:>6} {m['mem_peak']:>6} | {n['errors']}/{m['errors']} | {eff(n)} {eff(m)}")

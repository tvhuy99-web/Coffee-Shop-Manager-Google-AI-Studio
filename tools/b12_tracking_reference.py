#!/usr/bin/env python3
"""Measure two public Metrica games. No fit to match outcomes or inferred roles."""
import csv
import hashlib
import io
import json
import math
import statistics
import sys
import urllib.request
from pathlib import Path

SHA = "e706dd506b360d69d9d123d5b8026e7294b13996"
FILES = {
    (1, "Away"): "ff409a5680fd75acc7cebcf786827214c687478b",
    (1, "Home"): "ce2e91c0e633426cec1146f4e728b72417454e3d",
    (2, "Away"): "e7334d8d762954a215c49e55f211855000243fdc",
    (2, "Home"): "8b4d3648888cc8b033e7142919ebaea04f6328e0",
}


def quantiles(values):
    if not values:
        return {"n": 0}
    a = sorted(values)
    return {"n": len(a), "p50": statistics.median(a),
            "p90": a[math.ceil(.90*len(a))-1],
            "p95": a[math.ceil(.95*len(a))-1],
            "p99": a[math.ceil(.99*len(a))-1], "max": a[-1]}


def coords(row, idx):
    try:
        x, y = float(row[idx]), float(row[idx+1])
    except (ValueError, IndexError):
        return None
    if not (math.isfinite(x) and math.isfinite(y)):
        return None
    return x*105.0, y*68.0


def measure(file):
    reader = csv.reader(file)
    _, jerseys, header = (next(reader) for _ in range(3))
    assert [h.strip() for h in header[:3]] == ["Period", "Frame", "Time [s]"], header[:3]
    # Metrica: metadata, paired x/y per jersey, final ball pair.
    ball_col = next(i for i,h in enumerate(header) if h.strip().lower() in ("ball", "ball_x"))
    assert ball_col > 3 and (ball_col-3)%2==0, header
    columns = [(str(jerseys[i]).strip(), i) for i in range(3,ball_col,2)]
    assert columns and all(j for j,_ in columns) and len({j for j,_ in columns})==len(columns), jerseys
    players = {j: {"previous":None,"velocity":None,"speeds":[],"accel":[],
                   "tangent":[],"distance_m":0.0,"observed_s":0.0,
                   "missing_samples":0,"gaps":0,"outlier_intervals":0}
               for j,_ in columns}
    frames = 0
    for row in reader:
        if not row or len(row)<3:
            continue
        period, frame, t = int(row[0]),int(row[1]),float(row[2])
        if frame % 5:
            continue
        frames += 1
        for j,i in columns:
            p=players[j]
            pos=coords(row,i)
            prev=p["previous"]
            p["previous"]=(period,frame,t,pos) if pos is not None else None
            if pos is None:
                p["missing_samples"]+=1
                p["velocity"]=None
                continue
            if prev is None or prev[0]!=period or frame-prev[1]!=5 or abs(t-prev[2]-.2)>.005:
                p["gaps"]+=1
                p["velocity"]=None
                continue
            dt=t-prev[2]
            vx,vy=(pos[0]-prev[3][0])/dt,(pos[1]-prev[3][1])/dt
            speed=math.hypot(vx,vy)
            # Exclude suspect jumps; disclose count, never bridge them.
            if speed>12.0:
                p["outlier_intervals"]+=1
                p["velocity"]=None
                continue
            p["distance_m"]+=speed*dt
            p["observed_s"]+=dt
            p["speeds"].append(speed)
            old=p["velocity"]
            p["velocity"]=(vx,vy,speed)
            if old is not None:
                p["accel"].append(math.hypot(vx-old[0],vy-old[1])/dt)
                p["tangent"].append(abs(speed-old[2])/dt)
    assert frames>0
    rows=[]
    for j,p in players.items():
        rows.append({"jersey":j,"observed_s":p["observed_s"],"distance_m":p["distance_m"],
            "speed_mps":quantiles(p["speeds"]),"vector_acceleration_mps2":quantiles(p["accel"]),
            "absolute_tangential_acceleration_mps2":quantiles(p["tangent"]),
            "missing_samples":p["missing_samples"],"gaps":p["gaps"],"outlier_intervals":p["outlier_intervals"]})
    assert any(p["speed_mps"]["n"]>0 for p in rows)
    return {"sampled_frames":frames,"players":rows}


def self_test():
    rows = ["Team,,,Home,,,","Player,,,7,,,","Period,Frame,Time [s],Player7,,Ball,",
            "1,0,0,0.1,0.5,0,0","1,5,0.2,0.104,0.5,0,0","1,10,0.4,0.108,0.5,0,0",
            "1,15,0.6,NaN,NaN,0,0","1,20,0.8,0.2,0.5,0,0",
            "1,25,1.0,0.204,0.5,0,0","2,30,1.2,0.5,0.5,0,0",
            "2,35,1.4,0.9,0.5,0,0"]
    p=measure(io.StringIO("\n".join(rows)))["players"][0]
    assert p["speed_mps"]["n"]==3 and abs(p["speed_mps"]["p50"]-2.1)<1e-9
    assert p["missing_samples"]==1 and p["outlier_intervals"]==1
    assert p["vector_acceleration_mps2"]["n"]==1
    print("tracking parser synthetic gap, period, scale and outlier tests passed")


def main():
    if sys.argv[1:]==["--self-test"]:
        self_test()
        return
    if len(sys.argv)!=4 or sys.argv[1]!="--download":
        raise SystemExit("usage: --self-test OR --download CACHE OUTPUT")
    cache,output=map(Path,sys.argv[2:])
    cache.mkdir(parents=True,exist_ok=True)
    results=[]
    for (game,side),expected in FILES.items():
        name=f"Sample_Game_{game}_RawTrackingData_{side}_Team.csv"
        url=f"https://raw.githubusercontent.com/metrica-sports/sample-data/{SHA}/data/Sample_Game_{game}/{name}"
        target=cache/name
        with urllib.request.urlopen(url,timeout=90) as src, target.open("wb") as out:
            while chunk:=src.read(1024*1024):
                out.write(chunk)
        # Verify exact Git object, not just a mutable download URL.
        digest=hashlib.sha1(f"blob {target.stat().st_size}\0".encode())
        with target.open("rb") as f:
            while chunk:=f.read(1024*1024):
                digest.update(chunk)
        if digest.hexdigest()!=expected:
            raise ValueError(f"unexpected tracking blob {name}")
        with target.open(newline="",encoding="utf-8-sig") as f:
            result=measure(f)
        results.append({"game":game,"side":side,"url":url,"git_blob_sha":expected,**result})
    output.write_text(json.dumps({"schema":1,"provider":"Metrica Sports sample-data",
        "source_commit":SHA,"pitch_m":[105,68],"sampling_hz":5,"source_hz":25,
        "speed_outlier_mps":12,"games":results,
        "limitations":["Two anonymized games; no inferred playing role or attribute level.",
        "5 Hz endpoint displacement, not full path length; no interpolation across missing data.",
        "Speed and acceleration estimates depend on sampling and noise; 12 m/s jump filter disclosed.",
        "Includes stoppages; not directly comparable with live-only or upward crossing counters.",
        "Descriptive reference only, not a fit or proof of solver realism."]},indent=2,allow_nan=False))


if __name__=="__main__":
    main()

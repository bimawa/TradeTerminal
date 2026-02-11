import hmac, hashlib, time, requests, os

env = {}
with open(".env") as f:
    for line in f:
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            k, v = line.split("=", 1)
            env[k.strip()] = v.strip()

KEY = env["BINANCE_API_KEY"]
SECRET = env["BINANCE_API_SECRET"]
BASE = "https://fapi.binance.com"
H = {"X-MBX-APIKEY": KEY}


def sign(p):
    p["timestamp"] = int(time.time() * 1000)
    q = "&".join(f"{k}={v}" for k, v in p.items())
    s = hmac.new(SECRET.encode(), q.encode(), hashlib.sha256).hexdigest()
    return q + "&signature=" + s


r = requests.get(f"{BASE}/fapi/v2/positionRisk?{sign({})}", headers=H)
pos = [p for p in r.json() if float(p.get("positionAmt", 0)) != 0]
print(f"Open positions: {len(pos)}")

for p in pos:
    print(f"  {p['symbol']} {p['positionSide']} amt={p['positionAmt']}")
    amt = float(p["positionAmt"])
    side = "SELL" if amt > 0 else "BUY"
    params = {
        "symbol": p["symbol"],
        "side": side,
        "type": "MARKET",
        "quantity": str(abs(amt)),
        "positionSide": p["positionSide"],
    }
    r2 = requests.post(f"{BASE}/fapi/v1/order?{sign(params)}", headers=H)
    print(f"  Close: {r2.json()}")

r3 = requests.post(
    f"{BASE}/fapi/v1/positionSide/dual?{sign({'dualSidePosition': 'false'})}",
    headers=H,
)
print(f"Switch to One-Way mode: {r3.json()}")

# Routing Layer Design

## Overview

The routing layer is where OmniMesh becomes unique. It maintains a weighted
directed graph of peers and selects optimal paths based on real-time metrics.

## Route Graph

```
   A ──(5ms, 100Mbps)──▶ B ──(10ms, 50Mbps)──▶ D
   │                      │                      ▲
   │                      │                      │
   └──(20ms, 200Mbps)──▶ C ──(3ms, 80Mbps)───┘
```

Each edge carries metrics:
- **Latency** (ms) — round-trip time
- **Bandwidth** (Mbps) — available throughput
- **Packet loss** (%) — observed loss rate
- **Cost** — monetary or resource cost for relay
- **Hop count** — number of intermediate nodes

## Composite Cost Function

```
cost(edge) = latency_ms × (1 + loss_rate) / bandwidth_mbps + relay_cost
```

Lower cost = better path. Path finding uses modified Dijkstra/A* with
this composite metric.

## Multipath Routing

For high-throughput or high-reliability scenarios, OmniMesh can split
traffic across multiple paths:

- **Redundant mode:** Send on all paths (highest reliability)
- **Striped mode:** Round-robin across paths (highest throughput)
- **Primary/backup:** Use best path, failover on loss

## Relay Nodes

When direct connection is impossible (symmetric NAT, firewall):
1. Discover relay nodes via bootstrap/DHT
2. Establish encrypted tunnel through relay
3. Continue hole-punch attempts in background
4. Upgrade to direct when possible (transparent to application)

## Loop Prevention

- TTL field in every packet (decremented at each hop)
- Path vector: each packet carries visited-node list
- Split horizon: don't advertise a route back to its source

## Convergence

- Link-state updates propagated via gossip
- Exponential moving average for metric smoothing
- Dampening: don't flap routes on transient metric changes
- Maximum update rate: 1 update per second per link

## Performance Goals

- Route computation: < 1ms for 1000-node graph
- Convergence time: < 5s after topology change
- Memory: < 1MB for 10,000-peer routing table

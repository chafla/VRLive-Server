# VRLive Server

A server written in Rust for my Master's Capstone project. This server is designed to be the intermediary relay between performers
and audience members in a virtual reality performance, and also serves as an orchestrator for said performances.

Key components of the server include:
- Heavy multi-threading through message passing to ensure high throughput with little contention,
- A robust client-server listener model that is able to easily and consistently handle reconnections,
- A well-defined set of protocols for interacting with and exchanging both audio and motion capture data,

and so on and so forth.

## Organization

### [Protocol Definitions](protocol)

Defines most of the protocols in use, and the manners in which we interact with remote clients.

### [Server](server)

Includes the main logic for the server. In here, you can find the primary server component, as well as the client components,
responsible for spinning up the various listener tasks.

### [Utils](utils)

Contains some utilities I made use of while working on the server. Includes a sample Python client implementation, which was developed
alongside the server. It may be a little rough around the edges, but should be able to implement most of the base functionality of the server.
Also included is a simple opus streamer, which can be used to test the capabilities for working with Opus.

## Running

The server is fairly easy to start up. To spin up the server listening for handshakes on port 5653, simply call

```sh
cargo run --package vrlive-server --bin vrlive-server -- --name "test server" --port 5653 --host "0.0.0.0"
```
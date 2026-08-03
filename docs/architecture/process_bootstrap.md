# Process Bootstrap Architecture

> **Status:** Draft
> **Initiative:** v1.2.0 — Device Event and Ring-3 Runtime Completion

## 1. Program Charter
This document defines the capability handoff, resource provisioning, and structural boundaries required to bootstrap a new Ring-3 process in Gaxera.

## 2. Problem Statement
Currently, production services either rely on a dummy allocator or assume capability handles are present at hardcoded integer slot numbers. There is no formalized mechanism for a process creator to explicitly provision a `Factory`, an `AddressSpace`, a `CapabilitySpace`, a `Thread`, and link them to a charged `ResourceDomain`. Without this contract, services cannot safely migrate to the fallible Ring-3 heap because they lack the authority to request memory.

## 3. Non-Goals
- We are not building a POSIX `fork`/`exec` model.
- We are not implementing implicit dynamic capability discovery via a global namespace; all authority must be explicitly delegated.

## 4. Current Limitations
- The `init` process and subsequent servers use the legacy dummy allocator.
- Handle indices are guessed.
- ResourceDomain charging is not clearly transferred from the creator to the new process context in a generic way.

## 5. Candidate Authority Models
*(To be researched)*
- Explicit handle-passing via `StartProcess` payload vs. structured bootinfo page mapped into the new process's `AddressSpace`.

## 6. Capability Handoff Model
*(To be defined)*
How does the loader provision the root `Factory` and `AddressSpace` capabilities into the new `CapabilitySpace` before the process's primary `Thread` starts execution?

## 7. ResourceDomain Ownership Questions
Who owns the `ResourceDomain` of a new process? Does it draw from a parent's quota, or is it an independent top-level domain provisioned by the supervisor?

## 8. Process Exit Questions
When a process exits, how are its bootstrap capabilities revoked? Are they automatically destroyed or explicitly managed?

## 9. Supervisor Questions
How does a supervisor process trace, debug, or forcefully terminate a child process without ambient kernel authority?

## 10. Verification Expectations
- `test-bootstrap-handoff`: Process receives correct capabilities.
- `test-bootstrap-denial`: Process cannot access capabilities not explicitly handed off.
- `test-no-slot-guessing`: Verification that hardcoded slot indices are removed.

## 11. Dependencies
- Dependent on v1.1.0 Ring-3 Memory Foundation.
- Prerequisite for M4 Production Service Allocator Migration (v1.2).

## 12. Deferred Decisions
- Dynamic shared library loading is deferred until basic static executable bootstrap is formalized.

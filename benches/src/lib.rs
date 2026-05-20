/// Synthetic Gherkin feature content for parsing benchmarks.
/// Content is representative of the naia BDD suite scale and patterns.

/// Small feature: 3 scenarios, ~10 steps. Exercises the minimal parse path.
pub const FEATURE_SMALL: &str = r#"@Feature(lifecycle)
Feature: Connection lifecycle

  @Scenario(01)
  Scenario: Server observes connect event when client connects
    Given a server is running
    When a client connects
    Then the server has observed ConnectEvent

  @Scenario(02)
  Scenario: Server observes disconnect event when client disconnects
    Given a server is running
    And a client is connected
    When the client disconnects
    Then the server has observed DisconnectEvent

  @Scenario(03)
  Scenario: Client observes reject event when server rejects connection
    Given a server is running with max 0 users
    When a client attempts to connect
    Then the client has observed RejectEvent
"#;

/// Medium feature: 15 scenarios, ~80 steps. Exercises multi-scenario parse.
pub const FEATURE_MEDIUM: &str = r#"@Feature(messaging)
Feature: Message delivery between server and clients

  @Scenario(01)
  Scenario: Server sends a message to all connected clients
    Given a server is running
    And 3 clients are connected
    When the server sends a broadcast message
    Then all 3 clients receive the message

  @Scenario(02)
  Scenario: Server sends a message to a specific client
    Given a server is running
    And 2 clients are connected
    When the server sends a targeted message to client A
    Then client A receives the message
    And client B does not receive the message

  @Scenario(03)
  Scenario: Client sends a message to the server
    Given a server is running
    And a client is connected
    When the client sends a message
    Then the server receives the message

  @Scenario(04)
  Scenario: Message ordering is preserved for sequential sends
    Given a server is running
    And a client is connected
    When the client sends messages 1 2 3 in sequence
    Then the server receives them in order 1 2 3

  @Scenario(05)
  Scenario: Dropped packets are retransmitted reliably
    Given a server is running
    And a client is connected with packet loss at 50 percent
    When the client sends a reliable message
    Then the server eventually receives the message

  @Scenario(06)
  Scenario: Server broadcasts to room members
    Given a server is running
    And clients A B C are connected
    And clients A and B are in room alpha
    And client C is in room beta
    When the server broadcasts to room alpha
    Then client A receives the message
    And client B receives the message
    And client C does not receive the message

  @Scenario(07)
  Scenario: Message payload is delivered intact
    Given a server is running
    And a client is connected
    When the client sends a message with payload hello world
    Then the server receives a message with payload hello world

  @Scenario(08)
  Scenario: Multiple concurrent message senders
    Given a server is running
    And 5 clients are connected
    When all 5 clients send a message simultaneously
    Then the server receives 5 messages

  @Scenario(09)
  Scenario: Server rejects messages from disconnected clients
    Given a server is running
    And a client was connected but disconnected
    When the client attempts to send a message
    Then no message is delivered to the server

  @Scenario(10)
  Scenario: Large message payload delivery
    Given a server is running
    And a client is connected
    When the client sends a message with 1024 bytes of payload
    Then the server receives all 1024 bytes intact

  @Scenario(11)
  Scenario: Message delivery under simulated network jitter
    Given a server is running
    And a client is connected with 50ms jitter
    When the server sends 10 sequential messages
    Then the client receives all 10 messages

  @Scenario(12)
  Scenario: Server can receive messages from many clients in one tick
    Given a server is running
    And 16 clients are connected
    When each client sends 1 message
    Then after one tick the server has received 16 messages

  @Scenario(13)
  Scenario: Reconnected client can receive messages normally
    Given a server is running
    And a client connects disconnects and reconnects
    When the server sends a message
    Then the reconnected client receives it

  @Scenario(14)
  Scenario: Server can target messages to clients by user key
    Given a server is running
    And clients A B C are connected with user keys 1 2 3
    When the server sends a message to user key 2
    Then only client B receives the message

  @Scenario(15)
  Scenario: Zero-length messages are valid and deliverable
    Given a server is running
    And a client is connected
    When the client sends an empty message
    Then the server receives an empty message
"#;

/// Large feature: 40+ scenarios with backgrounds and outlines. Exercises full parse complexity.
pub const FEATURE_LARGE: &str = r#"@Feature(replication)
Feature: Entity replication from server to clients

  Background:
    Given a server is running
    And a client is connected

  @Scenario(01)
  Scenario: Newly spawned entity is replicated to client
    When the server spawns entity E with component Position 1 2
    And one tick passes
    Then the client has entity E with component Position 1 2

  @Scenario(02)
  Scenario: Component update is replicated to client
    Given the server has entity E with component Position 0 0
    And the client has entity E with component Position 0 0
    When the server updates entity E component Position to 5 10
    And one tick passes
    Then the client has entity E with component Position 5 10

  @Scenario(03)
  Scenario: Despawned entity is removed from client
    Given the server has entity E with component Position 0 0
    And the client has entity E with component Position 0 0
    When the server despawns entity E
    And one tick passes
    Then the client does not have entity E

  @Scenario(04)
  Scenario: Out-of-scope entity is removed from client
    Given the server has entity E visible to the client
    When the server removes entity E from client scope
    And one tick passes
    Then the client does not have entity E

  @Scenario(05)
  Scenario: In-scope entity is added to client when scope expands
    Given the server has entity E not visible to the client
    When the server adds entity E to client scope
    And one tick passes
    Then the client has entity E

  @Scenario(06)
  Scenario: Multiple components on same entity replicated correctly
    When the server spawns entity E with components Position 1 2 and Velocity 3 4
    And one tick passes
    Then the client has entity E with component Position 1 2
    And the client has entity E with component Velocity 3 4

  @Scenario(07)
  Scenario: Multiple entities replicated in single tick
    When the server spawns entities E1 E2 E3 with component Position 0 0
    And one tick passes
    Then the client has entities E1 E2 and E3

  @Scenario(08)
  Scenario: Component update only sends diff not full state
    Given the server has entity E with components A B C and D
    When only component B is updated
    And one tick passes
    Then the wire payload contains only the B component update

  @Scenario(09)
  Scenario: Rapid successive updates coalesce into single diff
    Given the server has entity E with component Position 0 0
    When the server updates Position to 1 0 then 2 0 then 3 0 in one tick
    And one tick passes
    Then the client sees only the final Position 3 0

  @Scenario(10)
  Scenario: Replication works with many entities
    When the server spawns 100 entities each with component Position at origin
    And one tick passes
    Then the client has 100 entities

  @Scenario(11)
  Scenario: Replication resumes after client reconnects
    Given the server has entity E with component Position 1 2
    And the client has entity E with component Position 1 2
    When the client disconnects and reconnects
    And one tick passes
    Then the client has entity E with component Position 1 2

  @Scenario(12)
  Scenario: Client does not receive updates for out-of-scope entity
    Given the server has entity E out of client scope
    When the server updates entity E component Position to 9 9
    And one tick passes
    Then the client does not have entity E

  @Scenario(13)
  Scenario: Scope grant then update arrives in correct order
    Given the server has entity E out of client scope with Position 1 1
    When the server adds E to scope and immediately updates Position to 2 2
    And one tick passes
    Then the client has entity E with component Position 2 2

  @Scenario(14)
  Scenario: Scope revoke removes entity even if update pending
    Given the server has entity E in client scope with Position 1 1
    When the server revokes scope and updates Position to 2 2 in same tick
    And one tick passes
    Then the client does not have entity E

  @Scenario(15)
  Scenario: Static entity never changes after initial replication
    Given the server spawns static entity E with component Position 1 2
    And the client has received entity E
    When 10 ticks pass with no updates
    Then the client still has entity E with component Position 1 2
    And no replication packets were sent in those 10 ticks

  @Scenario(16)
  Scenario: Immutable component skips diff tracking
    Given the server has entity E with immutable component Label foo
    When 5 ticks pass
    Then no diff packets are generated for entity E

  @Scenario(17)
  Scenario: Client observes SpawnEntity event
    When the server spawns entity E
    And one tick passes
    Then the client has observed SpawnEntity event for E

  @Scenario(18)
  Scenario: Client observes DespawnEntity event
    Given the server has entity E in client scope
    When the server despawns entity E
    And one tick passes
    Then the client has observed DespawnEntity event for E

  @Scenario(19)
  Scenario: Client observes UpdateComponent event
    Given the server has entity E with component Position 0 0 in client scope
    When the server updates Position to 1 1
    And one tick passes
    Then the client has observed UpdateComponent event for E Position

  @Scenario(20)
  Scenario: Replication order within a tick is deterministic
    When the server spawns entities E1 E2 E3 in order
    And one tick passes
    Then the client receives spawn events in order E1 E2 E3

  @Scenario(21)
  Scenario: Server side component update does not affect client until next tick
    Given the server has entity E with component Position 0 0 in client scope
    And the client has entity E with component Position 0 0
    When the server updates Position to 5 5
    Then without ticking the client still has Position 0 0

  @Scenario(22)
  Scenario: Multi-component spawn arrives atomically on client
    When the server spawns entity E with components A B C simultaneously
    And one tick passes
    Then the client receives all of A B C in the same event batch

  @Scenario(23)
  Scenario: 16 clients each see only their scoped entities
    Given the server has entities E1 through E16
    And each entity Ei is in scope only for client i
    When one tick passes
    Then client i has only entity Ei for each i in 1 through 16

  @Scenario(24)
  Scenario: Replication bandwidth is O of changed entities not total entities
    Given the server has 1000 entities in scope for one client
    When only 1 entity has a component update
    And one tick passes
    Then the wire payload size is proportional to 1 update not 1000

  @Scenario(25)
  Scenario: Entity visible to multiple clients replicates correctly to all
    Given the server has entity E in scope for clients A B and C
    When the server updates entity E component Position to 7 7
    And one tick passes
    Then client A has entity E with Position 7 7
    And client B has entity E with Position 7 7
    And client C has entity E with Position 7 7

  @Scenario(26)
  Scenario: Client with zero scope sees no entities
    Given the server has 10 entities
    And client A has no entities in scope
    When 5 ticks pass
    Then client A has no entities

  @Scenario(27)
  Scenario: Adding many entities to scope in one tick works
    Given the server has 50 entities not in client scope
    When all 50 entities are added to client scope simultaneously
    And one tick passes
    Then the client has all 50 entities

  @Scenario(28)
  Scenario: Removing many entities from scope in one tick works
    Given the server has 50 entities in client scope
    When all 50 entities are removed from scope simultaneously
    And one tick passes
    Then the client has no entities

  @Scenario(29)
  Scenario: Mixed spawn and despawn in same tick replicates correctly
    Given the server has entity E1 in scope
    When entity E1 is despawned and entity E2 is spawned in the same tick
    And one tick passes
    Then the client does not have E1
    And the client has E2

  @Scenario(30)
  Scenario: Reconnecting client gets full current scope state
    Given the server has 5 entities in scope for client A
    When client A disconnects and a new client B connects
    And client B is given the same scope
    And one tick passes
    Then client B has all 5 entities

  @Scenario(31)
  Scenario: Component with nested fields replicates all fields
    When the server spawns entity E with component ComplexState with nested fields
    And one tick passes
    Then the client has entity E with all nested fields correct

  @Scenario(32)
  Scenario: Replication survives 100 consecutive ticks without corruption
    Given the server has 10 entities in client scope each with Position
    When 100 ticks pass with random position updates each tick
    Then after 100 ticks all client entity positions match server positions

  @Scenario(33)
  Scenario: Scope policy applied at tick boundary not at API call time
    Given entity E is not in scope
    When scope is added mid-tick and entity is updated mid-tick
    And the tick completes
    Then the client receives both spawn and update atomically

  @Scenario(34)
  Scenario: Server can remove a component from an entity
    Given the server has entity E with components A and B
    When the server removes component B from entity E
    And one tick passes
    Then the client has entity E without component B

  @Scenario(35)
  Scenario: Removed component triggers RemoveComponent event on client
    Given the server has entity E with component A
    When the server removes component A from entity E
    And one tick passes
    Then the client has observed RemoveComponent event for E A

  @Scenario(36)
  Scenario: Client entity state is consistent after network disruption
    Given the server has entity E with Position 1 2 in scope
    And the client has entity E with Position 1 2
    When the network is disrupted for 3 ticks then restored
    And one tick passes after restoration
    Then the client has entity E with the latest server Position

  @Scenario(37)
  Scenario: Replication gate preserves order under concurrent updates
    Given 4 clients connected and entity E in scope for all
    When each tick 3 concurrent component updates are applied to E
    And 20 ticks pass
    Then all 4 clients have identical final state for E

  @Scenario(38)
  Scenario: Zero-component entity can be spawned and replicated
    When the server spawns entity E with no components
    And one tick passes
    Then the client has entity E with no components

  @Scenario(39)
  Scenario: Replication throughput does not degrade with 16 clients
    Given 16 clients connected all with 1000 entities in scope
    When 10 ticks pass with 100 updates per tick
    Then each tick completes in under the configured tick budget

  @Scenario(40)
  Scenario: Server crash recovery restores replication state
    Given the server has entity E in client scope with Position 3 4
    When the server restarts and the client reconnects
    And one tick passes
    Then the client has entity E with Position 3 4
"#;

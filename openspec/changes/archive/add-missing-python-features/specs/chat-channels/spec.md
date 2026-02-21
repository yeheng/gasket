## ADDED Requirements

### Requirement: Feishu Channel

The system SHALL support Feishu (飞书) as a chat channel.

#### Scenario: Configure Feishu channel

- **WHEN** the configuration includes:
  ```json
  {
    "channels": {
      "feishu": {
        "enabled": true,
        "appId": "${FEISHU_APP_ID}",
        "appSecret": "${FEISHU_APP_SECRET}",
        "allowFrom": ["ou_xxxxx"]
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a Feishu channel
- **AND** connect to Feishu WebSocket endpoint

#### Scenario: Receive Feishu message

- **WHEN** a user sends a message to the Feishu bot
- **THEN** the system SHALL receive the message via WebSocket
- **AND** parse the message format
- **AND** publish InboundMessage to the message bus

#### Scenario: Send Feishu message

- **WHEN** the agent generates a response for Feishu channel
- **THEN** the system SHALL send the message via Feishu API
- **AND** handle message formatting (Markdown support)

#### Scenario: Feishu authentication

- **WHEN** the Feishu channel starts
- **THEN** the system SHALL authenticate using App ID and Secret
- **AND** obtain access token
- **AND** establish WebSocket connection

### Requirement: DingTalk Channel

The system SHALL support DingTalk (钉钉) as a chat channel.

#### Scenario: Configure DingTalk channel

- **WHEN** the configuration includes:
  ```json
  {
    "channels": {
      "dingtalk": {
        "enabled": true,
        "clientId": "${DINGTALK_CLIENT_ID}",
        "clientSecret": "${DINGTALK_CLIENT_SECRET}",
        "allowFrom": ["user123"]
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a DingTalk channel
- **AND** connect to DingTalk Stream service

#### Scenario: Receive DingTalk message

- **WHEN** a user sends a message to the DingTalk bot
- **THEN** the system SHALL receive the message via Stream mode
- **AND** parse the message format
- **AND** publish InboundMessage to the message bus

#### Scenario: Send DingTalk message

- **WHEN** the agent generates a response for DingTalk channel
- **THEN** the system SHALL send the message via DingTalk API
- **AND** handle message formatting (Markdown support)

#### Scenario: DingTalk Stream connection

- **WHEN** the DingTalk channel starts
- **THEN** the system SHALL establish Stream connection
- **AND** subscribe to message events
- **AND** handle reconnection on disconnect

### Requirement: QQ Channel

The system SHALL support QQ as a chat channel.

#### Scenario: Configure QQ channel

- **WHEN** the configuration includes:
  ```json
  {
    "channels": {
      "qq": {
        "enabled": true,
        "appId": "${QQ_APP_ID}",
        "appSecret": "${QQ_APP_SECRET}",
        "allowFrom": ["user123"]
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a QQ channel
- **AND** connect to QQ bot API

#### Scenario: Receive QQ message

- **WHEN** a user sends a message to the QQ bot
- **THEN** the system SHALL receive the message via QQ API
- **AND** parse the message format
- **AND** publish InboundMessage to the message bus

#### Scenario: Send QQ message

- **WHEN** the agent generates a response for QQ channel
- **THEN** the system SHALL send the message via QQ API
- **AND** handle message formatting

#### Scenario: QQ authentication

- **WHEN** the QQ channel starts
- **THEN** the system SHALL authenticate using App ID and Secret
- **AND** obtain access token

### Requirement: WhatsApp Channel

The system SHALL support WhatsApp as a chat channel via bridge (optional feature).

#### Scenario: Configure WhatsApp channel

- **WHEN** the configuration includes:
  ```json
  {
    "channels": {
      "whatsapp": {
        "enabled": true,
        "bridgeUrl": "http://localhost:3000",
        "bridgeToken": "${WHATSAPP_BRIDGE_TOKEN}",
        "allowFrom": ["1234567890"]
      }
    }
  }
  ```
- **THEN** the system SHALL initialize a WhatsApp channel
- **AND** connect to the WhatsApp bridge via WebSocket

#### Scenario: WhatsApp QR login

- **WHEN** user runs `nanobot channels login` for WhatsApp
- **THEN** the system SHALL display QR code from bridge
- **AND** wait for authentication
- **AND** notify user when authenticated

#### Scenario: Receive WhatsApp message

- **WHEN** a user sends a message to the WhatsApp bot
- **THEN** the system SHALL receive the message via bridge
- **AND** parse the message format
- **AND** publish InboundMessage to the message bus

### Requirement: Channel Feature Flags

The system SHALL use feature flags for optional channels.

#### Scenario: Compile with Feishu support

- **WHEN** compiled with `--features feishu`
- **THEN** the Feishu channel implementation SHALL be included
- **AND** available for configuration

#### Scenario: Compile without Feishu support

- **WHEN** compiled without `--features feishu`
- **THEN** the Feishu channel SHALL not be included in binary
- **AND** Feishu configuration SHALL be ignored with warning

#### Scenario: List available channels

- **WHEN** user runs `nanobot channels status`
- **THEN** the system SHALL only show channels compiled with feature flags
- **AND** indicate which channels are available but not configured

### Requirement: Channel White List

The system SHALL support user whitelist for all channels.

#### Scenario: Check user permission

- **WHEN** a message is received from a user
- **THEN** the system SHALL check if user ID is in allowFrom list
- **AND** only process message if user is allowed
- **AND** log rejection for unauthorized users

#### Scenario: Empty whitelist (allow all)

- **WHEN** allowFrom is empty or not configured
- **THEN** the system SHALL accept messages from all users

#### Scenario: Wildcard in whitelist

- **WHEN** allowFrom includes "*"
- **THEN** the system SHALL accept messages from all users

### Requirement: Channel Health Monitoring

The system SHALL monitor channel connection health.

#### Scenario: Detect channel disconnection

- **WHEN** a channel WebSocket connection is lost
- **THEN** the system SHALL log the disconnection
- **AND** update channel status to "disconnected"
- **AND** attempt automatic reconnection

#### Scenario: Channel reconnection

- **WHEN** a channel reconnects after disconnect
- **THEN** the system SHALL log the reconnection
- **AND** update channel status to "connected"
- **AND** resume message processing

#### Scenario: Display channel status

- **WHEN** user runs `nanobot channels status`
- **THEN** the system SHALL show:
  - Channel name
  - Connection status (connected/disconnected)
  - Configuration status (configured/not configured)
  - API credential status (✓ has key / ✗ no key)

### Requirement: Message Format Adaptation

The system SHALL adapt messages for each channel's format requirements.

#### Scenario: Markdown to Feishu format

- **WHEN** sending a message with Markdown to Feishu
- **THEN** the system SHALL convert Markdown to Feishu card format
- **AND** preserve text formatting (bold, italic, code)

#### Scenario: Markdown to DingTalk format

- **WHEN** sending a message with Markdown to DingTalk
- **THEN** the system SHALL convert Markdown to DingTalk Markdown
- **AND** handle DingTalk-specific limitations

#### Scenario: Long message handling

- **WHEN** a message exceeds channel length limit
- **THEN** the system SHALL split the message
- **AND** send as multiple messages
- **AND** log the split operation

### Requirement: Channel Error Handling

The system SHALL handle channel-specific errors gracefully.

#### Scenario: Invalid credentials

- **WHEN** channel authentication fails due to invalid credentials
- **THEN** the system SHALL log the error
- **AND** mark channel as unavailable
- **AND** continue operating other channels

#### Scenario: Rate limiting

- **WHEN** a channel returns rate limit error
- **THEN** the system SHALL retry with exponential backoff
- **AND** log rate limit event
- **AND** queue outgoing messages

#### Scenario: Channel API error

- **WHEN** a channel API returns an error
- **THEN** the system SHALL log the error details
- **AND** notify user if message delivery failed
- **AND** continue processing other messages

### Requirement: Multi-Channel Gateway

The system SHALL support running multiple channels simultaneously.

#### Scenario: Start multiple channels

- **WHEN** user runs `nanobot gateway` with multiple channels configured
- **THEN** the system SHALL start all enabled channels in parallel
- **AND** route messages to appropriate channel based on destination

#### Scenario: Graceful shutdown

- **WHEN** user presses Ctrl+C during gateway operation
- **THEN** the system SHALL stop accepting new messages
- **AND** wait for pending messages to complete
- **AND** close all channel connections cleanly

#### Scenario: Channel isolation

- **WHEN** one channel fails or disconnects
- **THEN** other channels SHALL continue operating normally
- **AND** message bus SHALL route around failed channel

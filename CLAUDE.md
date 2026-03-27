# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Main Instructions

See @AGENTS.md for comprehensive information about this codebase.

## Claude Code-Specific Guidance

### Code Exploration

When exploring the codebase or searching for functionality:

- **Use Task tool with Explore agent** for architectural questions:
  - "Where are errors from the client handled?"
  - "How does the MQTT bridge work?"
  - "What files handle authentication?"

### Before Making Changes

1. Read relevant sections from @AGENTS.md first
2. For actor-related changes, review `design/thin-edge-actors-design.md`
3. Look at similar existing code for patterns

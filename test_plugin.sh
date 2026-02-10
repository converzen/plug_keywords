#!/bin/bash
# Test script for STDIO MCP server
# database_url":"postgresql://localhost:gmc_v2@postgres:5432/gmc_v2_01"
# "postgres://gmc_v2:gmc_v2@localhost:5432/gmc_v2_01"
# Test 1: Configure and list tools (should have composite names)
# Test 1: Configuration via initialize.experimental
RUST_LOG=debug
# Path to your MCP Server executable
MCP_SERVER="../../mcp/target/release/mcp-server-stdio"

$MCP_SERVER --get-plugin-schema --plugin target/release/libplug_keywords.so | jq


echo "1. Testing initialize with experimental.configure..."
(
  cat <<'EOF' | jq -c .
{ "jsonrpc":"2.0",
  "id":1,
  "method":"initialize",
  "params":{
    "protocolVersion":"2024-11-05",
    "clientInfo": {
      "name":"test-client",
      "version":"1.0.0"},
    "capabilities": {
      "experimental": {
        "configure": {
          "plugin_name" : "zeno", 
          "plugin_path":"./target/release",
          "plugin_config": {
	    "directory_path" : "./plugins/zeno/data/database.yaml"
          }
        }
      }
    }
  }
}
EOF

  sleep 0.2
  cat <<'EOF' | jq -c .
  { "jsonrpc":"2.0",
    "id":2,
    "method":"tools/list"
  }
EOF

  sleep 0.1
  cat <<'EOF' | jq -c .
  {
    "jsonrpc": "2.0",
    "id": 3,
    "method": "tools/call",
    "params": {
      "name": "zeno_get_product_morsel",
      "arguments": {
        "keywords": "Written encryptn"
      }
    }
  }
EOF

) | $MCP_SERVER 2>log.txt | jq '.'


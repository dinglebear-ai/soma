#!/usr/bin/env bash
# Validate the template plugin artifacts shipped by this repository.
set -uo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

CHECKS=0
PASSED=0
FAILED=0

PLUGIN_ROOT="${PLUGIN_ROOT:-plugins/example}"
CLAUDE_PLUGIN_JSON="${PLUGIN_ROOT}/.claude-plugin/plugin.json"
CODEX_PLUGIN_JSON="${PLUGIN_ROOT}/.codex-plugin/plugin.json"
GEMINI_EXTENSION_JSON="${PLUGIN_ROOT}/gemini-extension.json"
MCP_JSON="${PLUGIN_ROOT}/.mcp.json"
HOOKS_JSON="${PLUGIN_ROOT}/hooks/hooks.json"
SKILLS_DIR="${PLUGIN_ROOT}/skills"

check() {
  local test_name="$1"
  local test_cmd="$2"

  CHECKS=$((CHECKS + 1))
  printf 'Checking: %s... ' "${test_name}"

  if eval "${test_cmd}" >/dev/null 2>&1; then
    printf '%b\n' "${GREEN}PASS${NC}"
    PASSED=$((PASSED + 1))
    return 0
  fi

  printf '%b\n' "${RED}FAIL${NC}"
  FAILED=$((FAILED + 1))
  return 1
}

echo "=== Validating rmcp-template Plugin Layout ==="
echo "Plugin root: ${PLUGIN_ROOT}"
echo

check "jq is available" "command -v jq"

check "Claude plugin manifest exists" "test -f '${CLAUDE_PLUGIN_JSON}'"
check "Claude plugin manifest is valid JSON" "jq empty '${CLAUDE_PLUGIN_JSON}'"
check "Claude plugin name is example" "test \"\$(jq -er '.name' '${CLAUDE_PLUGIN_JSON}')\" = 'example'"
check "Claude plugin has no version field" "test \"\$(jq -er 'has(\"version\")' '${CLAUDE_PLUGIN_JSON}')\" = 'false'"
check "Claude plugin points to MCP config" "test \"\$(jq -er '.mcpServers' '${CLAUDE_PLUGIN_JSON}')\" = './.mcp.json'"
check "Claude plugin points to hooks config" "test \"\$(jq -er '.hooks' '${CLAUDE_PLUGIN_JSON}')\" = './hooks/hooks.json'"
check "Claude plugin points to skills directory" "test \"\$(jq -er '.skills' '${CLAUDE_PLUGIN_JSON}')\" = './skills'"
check "Claude plugin declares optional server_url userConfig" "jq -er '.userConfig.server_url.required == false and .userConfig.server_url.default == \"\"' '${CLAUDE_PLUGIN_JSON}'"
check "Claude plugin declares api_token as sensitive" "jq -er '.userConfig.api_token.sensitive == true' '${CLAUDE_PLUGIN_JSON}'"
check "Claude plugin declares no_auth toggle" "jq -er '.userConfig.no_auth.type == \"boolean\"' '${CLAUDE_PLUGIN_JSON}'"
check "Claude plugin declares auth_mode default" "jq -er '.userConfig.auth_mode.default == \"bearer\"' '${CLAUDE_PLUGIN_JSON}'"

check "Codex plugin manifest exists" "test -f '${CODEX_PLUGIN_JSON}'"
check "Codex plugin manifest is valid JSON" "jq empty '${CODEX_PLUGIN_JSON}'"
check "Codex plugin name is example-mcp" "test \"\$(jq -er '.name' '${CODEX_PLUGIN_JSON}')\" = 'example-mcp'"
check "Codex plugin has no version field" "test \"\$(jq -er 'has(\"version\")' '${CODEX_PLUGIN_JSON}')\" = 'false'"
check "Codex plugin points to MCP config" "test \"\$(jq -er '.mcpServers' '${CODEX_PLUGIN_JSON}')\" = './.mcp.json'"
check "Codex plugin points to skills directory" "test \"\$(jq -er '.skills' '${CODEX_PLUGIN_JSON}')\" = './skills/'"

check "Gemini extension manifest exists" "test -f '${GEMINI_EXTENSION_JSON}'"
check "Gemini extension manifest is valid JSON" "jq empty '${GEMINI_EXTENSION_JSON}'"
check "Gemini extension name is example-mcp" "test \"\$(jq -er '.name' '${GEMINI_EXTENSION_JSON}')\" = 'example-mcp'"
check "Gemini extension has no version field" "test \"\$(jq -er 'has(\"version\")' '${GEMINI_EXTENSION_JSON}')\" = 'false'"
check "Gemini extension points to skills directory" "test \"\$(jq -er '.skills' '${GEMINI_EXTENSION_JSON}')\" = './skills'"
check "Gemini MCP server is named example" "jq -er '.mcpServers.example' '${GEMINI_EXTENSION_JSON}'"
check "Gemini MCP transport is stdio" "jq -er '.mcpServers.example.type == \"stdio\"' '${GEMINI_EXTENSION_JSON}'"
check "Gemini MCP command uses bundled binary" "jq -er '.mcpServers.example.command == \"\${extensionPath}\${/}bin\${/}example\"' '${GEMINI_EXTENSION_JSON}'"
check "Gemini MCP args run stdio mode" "jq -er '.mcpServers.example.args == [\"mcp\"]' '${GEMINI_EXTENSION_JSON}'"
check "Gemini MCP env maps API URL" "jq -er '.mcpServers.example.env.EXAMPLE_API_URL == \"\${settings.example_api_url}\"' '${GEMINI_EXTENSION_JSON}'"
check "Gemini MCP env maps API key" "jq -er '.mcpServers.example.env.EXAMPLE_API_KEY == \"\${settings.example_api_key}\"' '${GEMINI_EXTENSION_JSON}'"

check "MCP config exists" "test -f '${MCP_JSON}'"
check "MCP config is valid JSON" "jq empty '${MCP_JSON}'"
check "MCP server is named example" "jq -er '.mcpServers.example' '${MCP_JSON}'"
check "MCP transport is stdio" "jq -er '.mcpServers.example.type == \"stdio\"' '${MCP_JSON}'"
check "MCP command uses bundled binary" "jq -er '.mcpServers.example.command == \"\${CLAUDE_PLUGIN_ROOT}/bin/example\"' '${MCP_JSON}'"
check "MCP args run stdio mode" "jq -er '.mcpServers.example.args == [\"mcp\"]' '${MCP_JSON}'"
check "MCP env maps API URL" "jq -er '.mcpServers.example.env.EXAMPLE_API_URL == \"\${user_config.example_api_url}\"' '${MCP_JSON}'"
check "MCP env maps API key" "jq -er '.mcpServers.example.env.EXAMPLE_API_KEY == \"\${user_config.example_api_key}\"' '${MCP_JSON}'"
check "Plugin packaged stdio MCP smoke passes" "PLUGIN_ROOT='${PLUGIN_ROOT}' bash scripts/check-plugin-stdio-smoke.sh"

check "hooks config exists" "test -f '${HOOKS_JSON}'"
check "hooks config is valid JSON" "jq empty '${HOOKS_JSON}'"
check "SessionStart runs plugin setup" "jq -er '.hooks.SessionStart[]?.hooks[]?.command == \"\${CLAUDE_PLUGIN_ROOT}/hooks/plugin-setup.sh\"' '${HOOKS_JSON}'"
check "ConfigChange runs plugin setup" "jq -er '.hooks.ConfigChange[]? | select(.matcher == \"user_settings\") | .hooks[]?.command == \"\${CLAUDE_PLUGIN_ROOT}/hooks/plugin-setup.sh\"' '${HOOKS_JSON}'"

check "skills directory exists" "test -d '${SKILLS_DIR}'"

skill_count=0
if [[ -d "${SKILLS_DIR}" ]]; then
  while IFS= read -r skill_file; do
    skill_count=$((skill_count + 1))
    skill_dir="$(basename "$(dirname "${skill_file}")")"
    check "skill ${skill_dir} has front matter name" "awk 'BEGIN {found=0} /^name:[[:space:]]*${skill_dir}[[:space:]]*$/ {found=1} END {exit found ? 0 : 1}' '${skill_file}'"
    check "skill ${skill_dir} has description" "awk 'BEGIN {found=0} /^description:[[:space:]]*[^[:space:]]/ {found=1} END {exit found ? 0 : 1}' '${skill_file}'"
  done < <(find "${SKILLS_DIR}" -mindepth 2 -maxdepth 2 -name SKILL.md | sort)
fi

CHECKS=$((CHECKS + 1))
printf 'Checking: at least one plugin skill exists... '
if (( skill_count > 0 )); then
  printf '%b\n' "${GREEN}PASS${NC}"
  PASSED=$((PASSED + 1))
else
  printf '%b\n' "${RED}FAIL${NC}"
  FAILED=$((FAILED + 1))
fi

echo
echo "=== Results ==="
echo "Total checks: ${CHECKS}"
printf '%b\n' "${GREEN}Passed: ${PASSED}${NC}"
if (( FAILED > 0 )); then
  printf '%b\n' "${RED}Failed: ${FAILED}${NC}"
  exit 1
fi

printf '%b\n' "${GREEN}All checks passed.${NC}"

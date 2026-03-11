"""Integration tests for the running memory service (Rust/LanceDB) via MCP-lite socket."""

import pytest
import json
from pathlib import Path
from openagent.platforms.mcplite import McpLiteClient


@pytest.mark.asyncio
async def test_memory_via_socket() -> None:
    """Connect directly to the memory service socket and invoke its tools.
    
    This test expects the memory service to already be running (e.g. via `make local`
    and `OPENAGENT_SOCKET_PATH=data/sockets/memory.sock ./bin/memory-darwin-arm64`).
    """
    root = Path(__file__).parent.parent.parent
    socket_path = root / "data" / "sockets" / "memory.sock"
    
    if not socket_path.exists():
        print("error memory service not running")
        pytest.fail("error memory service not running")
        
    client = McpLiteClient(socket_path=socket_path)
    
    try:
        await client.start()
        
        # 1. Test ping
        ping_resp = await client.request({"type": "ping"}, timeout_s=2.0)
        assert ping_resp.type == "pong"
        assert ping_resp.status == "ready"
        
        # 2. List tools to verify memory exposes index_trace and search_memory
        tools_resp = await client.request({"type": "tools.list"}, timeout_s=2.0)
        assert tools_resp.type == "tools.list.ok"
        tool_names = [t.name for t in tools_resp.tools]
        assert "memory.index" in tool_names
        assert "memory.search" in tool_names
        assert "memory.delete" in tool_names
        
        # 3. Index a test trace into LTS
        index_args = {
            "content": "pytest memory socket integration test content",
            "store": "ltm"
        }
        index_resp = await client.request({
            "type": "tool.call",
            "tool": "memory.index",
            "params": index_args
        }, timeout_s=10.0)
        
        assert index_resp.type == "tool.result"
        assert index_resp.error is None
        assert "id" in index_resp.result
        
        # 4. Search for the content
        search_args = {
            "query": "pytest memory socket",
            "store": "ltm"
        }
        search_resp = await client.request({
            "type": "tool.call",
            "tool": "memory.search",
            "params": search_args
        }, timeout_s=10.0)
        
        assert search_resp.type == "tool.result"
        assert search_resp.error is None
        assert "pytest memory socket integration test content" in search_resp.result
        
        # 5. Hybrid search with specialized keyword
        keyword_content = "The elusive pink-spotted-giraffe lives in the savanna."
        await client.request({
            "type": "tool.call",
            "tool": "memory.index",
            "params": {"content": keyword_content, "store": "stm"}
        }, timeout_s=10.0)
        
        search_res = await client.request({
            "type": "tool.call",
            "tool": "memory.search",
            "params": {"query": "pink-spotted-giraffe", "store": "all"}
        }, timeout_s=10.0)
        
        results = json.loads(search_res.result)
        # Verify that the keyword-matched document is top and has FTS metadata
        assert any(keyword_content in r['content'] for r in results)
        top_hit = results[0]
        assert "rrf_score" in top_hit
        assert "fts_rank" in top_hit
        assert top_hit["fts_rank"] > 0
        
    finally:
        await client.stop()

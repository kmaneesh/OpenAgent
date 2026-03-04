package main

import (
	"bufio"
	"context"
	"encoding/json"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"github.com/kmaneesh/openagent/services/sdk-go/mcplite"
)

func TestHandleShellExecArgv(t *testing.T) {
	rt := &runtimeConfig{root: t.TempDir(), maxOutputBytes: 4096}
	res := rt.handleShellExec(map[string]any{"argv": []any{"echo", "hello"}})
	if !res.OK {
		t.Fatalf("expected ok result, got %+v", res)
	}
	if res.ExitCode != 0 {
		t.Fatalf("expected zero exit code, got %d", res.ExitCode)
	}
}

func TestHandlePythonRunCode(t *testing.T) {
	if _, err := exec.LookPath("python3"); err != nil {
		t.Skip("python3 not available")
	}
	rt := &runtimeConfig{root: t.TempDir(), maxOutputBytes: 4096}
	res := rt.handlePythonRun(map[string]any{"code": "print('ok')"})
	if !res.OK {
		t.Fatalf("expected ok result, got %+v", res)
	}
	if res.ExitCode != 0 {
		t.Fatalf("expected zero exit code, got %d", res.ExitCode)
	}
}

func TestResolveCWDRejectsEscape(t *testing.T) {
	rt := &runtimeConfig{root: t.TempDir(), maxOutputBytes: 4096}
	_, _, err := rt.resolveCWD("../../")
	if err == nil {
		t.Fatal("expected escape rejection")
	}
}

func TestBuildServerHandlesToolCall(t *testing.T) {
	rt := &runtimeConfig{root: t.TempDir(), maxOutputBytes: 4096}
	server := buildServer(rt)
	resp, err := server.HandleRequest(context.Background(), mcplite.ToolCallRequest{
		ID:   "1",
		Type: mcplite.TypeToolCall,
		Tool: "shell.exec",
		Params: map[string]any{
			"argv": []any{"echo", "hello"},
		},
	})
	if err != nil {
		t.Fatalf("HandleRequest error: %v", err)
	}
	result, ok := resp.(mcplite.ToolResultResponse)
	if !ok || result.Result == nil {
		t.Fatalf("unexpected response: %+v", resp)
	}
}

func TestHandleConnPingRoundTrip(t *testing.T) {
	rt := &runtimeConfig{root: t.TempDir(), maxOutputBytes: 4096}
	server := buildServer(rt)
	left, right := net.Pipe()
	defer left.Close()
	defer right.Close()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go handleConn(ctx, left, server)

	_, err := right.Write([]byte(`{"id":"1","type":"ping"}` + "\n"))
	if err != nil {
		t.Fatalf("write ping failed: %v", err)
	}
	_ = right.SetReadDeadline(time.Now().Add(2 * time.Second))
	line, err := bufio.NewReader(right).ReadString('\n')
	if err != nil {
		t.Fatalf("read response failed: %v", err)
	}
	var raw map[string]any
	if err := json.Unmarshal([]byte(line), &raw); err != nil {
		t.Fatalf("json decode failed: %v", err)
	}
	if raw["type"] != "pong" {
		t.Fatalf("expected pong response, got %+v", raw)
	}
}

func TestHandlePythonRunScriptPath(t *testing.T) {
	if _, err := exec.LookPath("python3"); err != nil {
		t.Skip("python3 not available")
	}
	root := t.TempDir()
	script := filepath.Join(root, "a.py")
	if err := os.WriteFile(script, []byte("print('script-ok')\n"), 0o644); err != nil {
		t.Fatalf("write script: %v", err)
	}
	rt := &runtimeConfig{root: root, maxOutputBytes: 4096}
	res := rt.handlePythonRun(map[string]any{"script_path": "a.py"})
	if !res.OK {
		t.Fatalf("expected ok result, got %+v", res)
	}
}

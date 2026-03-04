package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
	"unicode/utf8"

	"github.com/kmaneesh/openagent/services/sdk-go/mcplite"
)

const (
	defaultSocketPath     = "data/sockets/shell.sock"
	defaultTimeoutSeconds = 30.0
	maxTimeoutSeconds     = 300.0
	defaultMaxOutputBytes = 64 * 1024
	maxOutputBytesHardCap = 1 * 1024 * 1024
)

type runtimeConfig struct {
	root           string
	maxOutputBytes int
}

type execResult struct {
	OK              bool     `json:"ok"`
	Command         []string `json:"command"`
	CWD             string   `json:"cwd"`
	ExitCode        int      `json:"exit_code"`
	TimedOut        bool     `json:"timed_out"`
	DurationMS      float64  `json:"duration_ms"`
	Stdout          string   `json:"stdout"`
	Stderr          string   `json:"stderr"`
	TruncatedStdout bool     `json:"truncated_stdout"`
	TruncatedStderr bool     `json:"truncated_stderr"`
	Error           string   `json:"error,omitempty"`
}

func main() {
	if err := run(); err != nil {
		log.Fatalf("shell service failed: %v", err)
	}
}

func run() error {
	ctx, cancel := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer cancel()

	rt, err := newRuntimeConfig()
	if err != nil {
		return err
	}

	socketPath := os.Getenv("OPENAGENT_SOCKET_PATH")
	if socketPath == "" {
		socketPath = defaultSocketPath
	}

	if err := os.MkdirAll(filepath.Dir(socketPath), 0o755); err != nil {
		return fmt.Errorf("create socket directory: %w", err)
	}
	if err := os.Remove(socketPath); err != nil && !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("remove stale socket: %w", err)
	}

	listener, err := net.Listen("unix", socketPath)
	if err != nil {
		return fmt.Errorf("listen on socket %q: %w", socketPath, err)
	}
	defer func() {
		_ = listener.Close()
		_ = os.Remove(socketPath)
	}()

	mcplite.LogEvent("INFO", "service listening", map[string]any{
		"service":          "shell",
		"socket_path":      socketPath,
		"root":             rt.root,
		"max_output_bytes": rt.maxOutputBytes,
	})

	server := buildServer(rt)
	var connWG sync.WaitGroup

	go func() {
		<-ctx.Done()
		_ = listener.Close()
	}()

	for {
		conn, acceptErr := listener.Accept()
		if acceptErr != nil {
			if errors.Is(acceptErr, net.ErrClosed) || ctx.Err() != nil {
				break
			}
			mcplite.LogEvent("ERROR", "accept failed", map[string]any{"service": "shell", "error": acceptErr.Error()})
			continue
		}
		connWG.Add(1)
		go func(c net.Conn) {
			defer connWG.Done()
			handleConn(ctx, c, server)
		}(conn)
	}

	connWG.Wait()
	return nil
}

func newRuntimeConfig() (*runtimeConfig, error) {
	root := strings.TrimSpace(os.Getenv("OPENAGENT_SHELL_ROOT"))
	if root == "" {
		cwd, err := os.Getwd()
		if err != nil {
			return nil, err
		}
		root = cwd
	}
	abs, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	st, err := os.Stat(abs)
	if err != nil {
		return nil, err
	}
	if !st.IsDir() {
		return nil, errors.New("OPENAGENT_SHELL_ROOT must be a directory")
	}

	maxOutput := intParamFromEnv("OPENAGENT_SHELL_MAX_OUTPUT_BYTES", defaultMaxOutputBytes)
	maxOutput = clampInt(maxOutput, 1024, maxOutputBytesHardCap)

	return &runtimeConfig{root: filepath.Clean(abs), maxOutputBytes: maxOutput}, nil
}

func buildServer(rt *runtimeConfig) *mcplite.Server {
	tools := []mcplite.ToolDefinition{
		{
			Name:        "shell.exec",
			Description: "Execute a shell command or argv with timeout and bounded output.",
			Params: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"command":   map[string]any{"type": "string", "description": "Command string. Use with use_shell=true or simple whitespace split."},
					"argv":      map[string]any{"type": "array", "items": map[string]any{"type": "string"}, "description": "Preferred explicit argv; first item is executable."},
					"use_shell": map[string]any{"type": "boolean", "description": "Run command through /bin/sh -lc. Default false."},
					"cwd":       map[string]any{"type": "string", "description": "Working directory relative to service root."},
					"timeout_s": map[string]any{"type": "number", "description": "Execution timeout seconds (default 30, max 300)."},
				},
			},
		},
		{
			Name:        "python.run",
			Description: "Execute Python code or script path via python3 with timeout and bounded output.",
			Params: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"code":        map[string]any{"type": "string", "description": "Python code string to run with -c."},
					"script_path": map[string]any{"type": "string", "description": "Path to script file relative to service root."},
					"args":        map[string]any{"type": "array", "items": map[string]any{"type": "string"}, "description": "Arguments passed to python code/script."},
					"python_bin":  map[string]any{"type": "string", "description": "Python executable name/path. Default python3."},
					"cwd":         map[string]any{"type": "string", "description": "Working directory relative to service root."},
					"timeout_s":   map[string]any{"type": "number", "description": "Execution timeout seconds (default 30, max 300)."},
				},
			},
		},
	}

	server := mcplite.NewServer(tools, "ready")
	server.RegisterToolHandler("shell.exec", func(_ context.Context, params map[string]any) (string, error) {
		res := rt.handleShellExec(params)
		return marshalAny(res)
	})
	server.RegisterToolHandler("python.run", func(_ context.Context, params map[string]any) (string, error) {
		res := rt.handlePythonRun(params)
		return marshalAny(res)
	})
	return server
}

func (rt *runtimeConfig) resolveCWD(raw string) (string, string, error) {
	if strings.TrimSpace(raw) == "" {
		return rt.root, ".", nil
	}
	candidate := raw
	if !filepath.IsAbs(candidate) {
		candidate = filepath.Join(rt.root, candidate)
	}
	abs, err := filepath.Abs(candidate)
	if err != nil {
		return "", "", err
	}
	if err := ensureWithinRoot(rt.root, abs); err != nil {
		return "", "", err
	}
	st, err := os.Stat(abs)
	if err != nil {
		return "", "", err
	}
	if !st.IsDir() {
		return "", "", errors.New("cwd is not a directory")
	}
	rel, _ := filepath.Rel(rt.root, abs)
	return abs, filepath.ToSlash(rel), nil
}

func (rt *runtimeConfig) handleShellExec(params map[string]any) execResult {
	argv := parseStringSlice(params["argv"])
	command := strings.TrimSpace(stringParam(params, "command", ""))
	useShell := boolParam(params, "use_shell", false)
	if len(argv) == 0 {
		if command == "" {
			return execResult{OK: false, ExitCode: -1, Error: "provide argv or command"}
		}
		if useShell {
			argv = []string{"/bin/sh", "-lc", command}
		} else {
			argv = strings.Fields(command)
		}
	}
	cwdAbs, cwdRel, err := rt.resolveCWD(stringParam(params, "cwd", ""))
	if err != nil {
		return execResult{OK: false, ExitCode: -1, Error: err.Error()}
	}
	timeout := clampFloat(floatParam(params, "timeout_s", defaultTimeoutSeconds), 1.0, maxTimeoutSeconds)
	return runCommand(argv, cwdAbs, cwdRel, timeout, rt.maxOutputBytes)
}

func (rt *runtimeConfig) handlePythonRun(params map[string]any) execResult {
	pythonBin := strings.TrimSpace(stringParam(params, "python_bin", "python3"))
	code := stringParam(params, "code", "")
	scriptPath := strings.TrimSpace(stringParam(params, "script_path", ""))
	args := parseStringSlice(params["args"])

	var argv []string
	if code != "" {
		argv = append([]string{pythonBin, "-c", code}, args...)
	} else if scriptPath != "" {
		absScript, _, err := rt.resolvePathAllowMissing(scriptPath, false)
		if err != nil {
			return execResult{OK: false, ExitCode: -1, Error: err.Error()}
		}
		argv = append([]string{pythonBin, absScript}, args...)
	} else {
		return execResult{OK: false, ExitCode: -1, Error: "provide code or script_path"}
	}

	cwdAbs, cwdRel, err := rt.resolveCWD(stringParam(params, "cwd", ""))
	if err != nil {
		return execResult{OK: false, ExitCode: -1, Error: err.Error()}
	}
	timeout := clampFloat(floatParam(params, "timeout_s", defaultTimeoutSeconds), 1.0, maxTimeoutSeconds)
	return runCommand(argv, cwdAbs, cwdRel, timeout, rt.maxOutputBytes)
}

func (rt *runtimeConfig) resolvePathAllowMissing(raw string, allowMissing bool) (string, string, error) {
	candidate := raw
	if !filepath.IsAbs(candidate) {
		candidate = filepath.Join(rt.root, candidate)
	}
	abs, err := filepath.Abs(candidate)
	if err != nil {
		return "", "", err
	}
	if err := ensureWithinRoot(rt.root, abs); err != nil {
		return "", "", err
	}
	if !allowMissing {
		if _, err := os.Stat(abs); err != nil {
			return "", "", err
		}
	}
	rel, _ := filepath.Rel(rt.root, abs)
	return abs, filepath.ToSlash(rel), nil
}

func runCommand(argv []string, cwdAbs, cwdRel string, timeoutS float64, maxOutput int) execResult {
	if len(argv) == 0 {
		return execResult{OK: false, ExitCode: -1, Error: "empty command"}
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(timeoutS*float64(time.Second)))
	defer cancel()

	start := time.Now()
	cmd := exec.CommandContext(ctx, argv[0], argv[1:]...)
	cmd.Dir = cwdAbs
	var stdoutBuf, stderrBuf bytes.Buffer
	cmd.Stdout = &stdoutBuf
	cmd.Stderr = &stderrBuf

	err := cmd.Run()
	durationMs := float64(time.Since(start).Microseconds()) / 1000.0

	stdout, truncOut := truncateUTF8(stdoutBuf.String(), maxOutput)
	stderr, truncErr := truncateUTF8(stderrBuf.String(), maxOutput)

	result := execResult{
		OK:              err == nil,
		Command:         argv,
		CWD:             cwdRel,
		ExitCode:        0,
		TimedOut:        errors.Is(ctx.Err(), context.DeadlineExceeded),
		DurationMS:      durationMs,
		Stdout:          stdout,
		Stderr:          stderr,
		TruncatedStdout: truncOut,
		TruncatedStderr: truncErr,
	}

	if err == nil {
		return result
	}
	result.Error = err.Error()
	result.ExitCode = extractExitCode(err)
	return result
}

func extractExitCode(err error) int {
	var ee *exec.ExitError
	if errors.As(err, &ee) {
		if ws, ok := ee.Sys().(syscall.WaitStatus); ok {
			return ws.ExitStatus()
		}
		return 1
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return 124
	}
	return -1
}

func ensureWithinRoot(root, path string) error {
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return err
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(os.PathSeparator)) {
		return fmt.Errorf("path escapes root: %s", path)
	}
	return nil
}

func truncateUTF8(s string, maxBytes int) (string, bool) {
	if len(s) <= maxBytes {
		return s, false
	}
	cut := s[:maxBytes]
	for !utf8ValidString(cut) && len(cut) > 0 {
		cut = cut[:len(cut)-1]
	}
	return cut, true
}

func utf8ValidString(s string) bool {
	return []byte(s) != nil && utf8.ValidString(s)
}

func handleConn(ctx context.Context, conn net.Conn, server *mcplite.Server) {
	defer conn.Close()

	decoder := mcplite.NewDecoder(conn)
	encoder := mcplite.NewEncoder(conn)
	var writeMu sync.Mutex
	var reqWG sync.WaitGroup

	for {
		frame, err := decoder.Next()
		if err != nil {
			if errors.Is(err, io.EOF) {
				break
			}
			mcplite.LogEvent("ERROR", "decode frame failed", map[string]any{"service": "shell", "error": err.Error()})
			break
		}

		reqWG.Add(1)
		go func(f mcplite.Frame) {
			defer reqWG.Done()
			start := time.Now()
			requestID := mcplite.RequestIDFromFrame(f)
			tool := mcplite.ToolNameFromFrame(f)
			outcome := "ok"

			response, handleErr := server.HandleRequest(ctx, f)
			if handleErr != nil {
				outcome = "error"
				id := frameID(f)
				if id == "" {
					mcplite.LogEvent("WARN", "unsupported frame", map[string]any{"service": "shell", "frame": fmt.Sprintf("%T", f)})
					return
				}
				response = mcplite.ErrorResponse{ID: id, Type: mcplite.TypeError, Code: "BAD_REQUEST", Message: handleErr.Error()}
			}

			writeMu.Lock()
			defer writeMu.Unlock()
			if err := encoder.WriteFrame(response); err != nil {
				outcome = "error"
				mcplite.LogEvent("ERROR", "write frame failed", map[string]any{"service": "shell", "request_id": requestID, "tool": tool, "error": err.Error()})
				return
			}
			mcplite.LogEvent("INFO", "request handled", map[string]any{"service": "shell", "request_id": requestID, "tool": tool, "outcome": outcome, "duration_ms": float64(time.Since(start).Microseconds()) / 1000.0})
		}(frame)
	}

	reqWG.Wait()
}

func frameID(frame mcplite.Frame) string {
	switch v := frame.(type) {
	case mcplite.ToolListRequest:
		return v.ID
	case mcplite.ToolCallRequest:
		return v.ID
	case mcplite.PingRequest:
		return v.ID
	default:
		return ""
	}
}

func marshalAny(v any) (string, error) {
	data, err := json.Marshal(v)
	if err != nil {
		return "", err
	}
	return string(data), nil
}

func stringParam(params map[string]any, key, fallback string) string {
	raw, ok := params[key]
	if !ok || raw == nil {
		return fallback
	}
	s, ok := raw.(string)
	if !ok {
		return fallback
	}
	return s
}

func parseStringSlice(raw any) []string {
	items, ok := raw.([]any)
	if !ok {
		return nil
	}
	out := make([]string, 0, len(items))
	for _, item := range items {
		s, ok := item.(string)
		if !ok {
			continue
		}
		s = strings.TrimSpace(s)
		if s == "" {
			continue
		}
		out = append(out, s)
	}
	return out
}

func intParamFromEnv(key string, fallback int) int {
	raw := strings.TrimSpace(os.Getenv(key))
	if raw == "" {
		return fallback
	}
	v, err := strconv.Atoi(raw)
	if err != nil {
		return fallback
	}
	return v
}

func floatParam(params map[string]any, key string, fallback float64) float64 {
	raw, ok := params[key]
	if !ok || raw == nil {
		return fallback
	}
	switch v := raw.(type) {
	case float64:
		return v
	case int:
		return float64(v)
	case int64:
		return float64(v)
	case string:
		p, err := strconv.ParseFloat(v, 64)
		if err != nil {
			return fallback
		}
		return p
	default:
		return fallback
	}
}

func boolParam(params map[string]any, key string, fallback bool) bool {
	raw, ok := params[key]
	if !ok || raw == nil {
		return fallback
	}
	switch v := raw.(type) {
	case bool:
		return v
	case string:
		b, err := strconv.ParseBool(v)
		if err != nil {
			return fallback
		}
		return b
	default:
		return fallback
	}
}

func clampInt(v, minV, maxV int) int {
	if v < minV {
		return minV
	}
	if v > maxV {
		return maxV
	}
	return v
}

func clampFloat(v, minV, maxV float64) float64 {
	if v < minV {
		return minV
	}
	if v > maxV {
		return maxV
	}
	return v
}

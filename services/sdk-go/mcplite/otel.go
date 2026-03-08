// Package mcplite — OTEL file-based tracing for Go MCP-lite services.
//
// SetupOTEL initialises a TracerProvider that writes spans as OTLP/JSON to:
//
//	<logsDir>/<serviceName>-traces-YYYY-MM-DD.jsonl
//
// Each line is one OTLP ExportTraceServiceRequest JSON object (OTLP/JSON spec).
// Files older than 1 day are deleted on rotation.
//
// To enable, add to the service's go.mod and run go mod tidy:
//
//	go.opentelemetry.io/otel           v1.35.0
//	go.opentelemetry.io/otel/sdk       v1.35.0
//
// Then call in main():
//
//	shutdown, err := mcplite.SetupOTEL("whatsapp", "logs")
//	if err == nil { defer shutdown(context.Background()) }
package mcplite

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"time"

	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	sdkresource "go.opentelemetry.io/otel/sdk/resource"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	semconv "go.opentelemetry.io/otel/semconv/v1.26.0"
	oteltrace "go.opentelemetry.io/otel/trace"
)

// ---------------------------------------------------------------------------
// OTLP/JSON span exporter
// ---------------------------------------------------------------------------

type otlpJsonExporter struct {
	mu      sync.Mutex
	dir     string
	prefix  string
	curDate string
	file    *os.File
}

func newOTLPJsonExporter(serviceName, logsDir string) (*otlpJsonExporter, error) {
	if err := os.MkdirAll(logsDir, 0o755); err != nil {
		return nil, fmt.Errorf("create logs dir: %w", err)
	}
	e := &otlpJsonExporter{dir: logsDir, prefix: serviceName + "-traces"}
	if err := e.rotateUnlocked(); err != nil {
		return nil, err
	}
	return e, nil
}

func (e *otlpJsonExporter) filename(d string) string {
	return filepath.Join(e.dir, e.prefix+"-"+d+".jsonl")
}

func (e *otlpJsonExporter) rotateUnlocked() error {
	today := time.Now().UTC().Format("2006-01-02")
	if today == e.curDate && e.file != nil {
		return nil
	}
	if e.file != nil {
		_ = e.file.Close()
	}
	f, err := os.OpenFile(e.filename(today), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return fmt.Errorf("open trace file: %w", err)
	}
	e.file = f
	e.curDate = today
	e.purgeOld(today)
	return nil
}

func (e *otlpJsonExporter) purgeOld(today string) {
	entries, _ := os.ReadDir(e.dir)
	prefix := e.prefix + "-"
	todayT, _ := time.Parse("2006-01-02", today)
	for _, ent := range entries {
		name := ent.Name()
		if len(name) < len(prefix)+12 {
			continue
		}
		if name[:len(prefix)] != prefix {
			continue
		}
		dateStr := name[len(prefix) : len(prefix)+10]
		fileDate, err := time.Parse("2006-01-02", dateStr)
		if err != nil {
			continue
		}
		if todayT.Sub(fileDate) > 24*time.Hour {
			_ = os.Remove(filepath.Join(e.dir, name))
		}
	}
}

// ExportSpans serialises spans as OTLP/JSON lines and appends to the daily file.
func (e *otlpJsonExporter) ExportSpans(ctx context.Context, spans []sdktrace.ReadOnlySpan) error {
	if len(spans) == 0 {
		return nil
	}
	e.mu.Lock()
	defer e.mu.Unlock()
	if err := e.rotateUnlocked(); err != nil {
		return err
	}
	payload := buildOTLPJSON(spans)
	line, err := json.Marshal(payload)
	if err != nil {
		return fmt.Errorf("marshal spans: %w", err)
	}
	_, err = fmt.Fprintf(e.file, "%s\n", line)
	return err
}

func (e *otlpJsonExporter) Shutdown(ctx context.Context) error {
	e.mu.Lock()
	defer e.mu.Unlock()
	if e.file != nil {
		err := e.file.Close()
		e.file = nil
		return err
	}
	return nil
}

// ---------------------------------------------------------------------------
// OTLP/JSON serialisation
// ---------------------------------------------------------------------------

type otlpExportRequest struct {
	ResourceSpans []otlpResourceSpans `json:"resourceSpans"`
}

type otlpResourceSpans struct {
	Resource   otlpResource   `json:"resource"`
	ScopeSpans []otlpScopeSpans `json:"scopeSpans"`
}

type otlpResource struct {
	Attributes []otlpKV `json:"attributes"`
}

type otlpScopeSpans struct {
	Scope otlpScope  `json:"scope"`
	Spans []otlpSpan `json:"spans"`
}

type otlpScope struct {
	Name    string `json:"name"`
	Version string `json:"version,omitempty"`
}

type otlpSpan struct {
	TraceID           string    `json:"traceId"`
	SpanID            string    `json:"spanId"`
	ParentSpanID      string    `json:"parentSpanId,omitempty"`
	Name              string    `json:"name"`
	Kind              int       `json:"kind"`
	StartTimeUnixNano string    `json:"startTimeUnixNano"`
	EndTimeUnixNano   string    `json:"endTimeUnixNano"`
	Attributes        []otlpKV  `json:"attributes"`
	Events            []otlpEvent `json:"events"`
	Status            otlpStatus `json:"status"`
}

type otlpEvent struct {
	TimeUnixNano string   `json:"timeUnixNano"`
	Name         string   `json:"name"`
	Attributes   []otlpKV `json:"attributes"`
}

type otlpStatus struct {
	Code    int    `json:"code"`
	Message string `json:"message,omitempty"`
}

type otlpKV struct {
	Key   string    `json:"key"`
	Value otlpValue `json:"value"`
}

type otlpValue struct {
	StringValue *string  `json:"stringValue,omitempty"`
	IntValue    *string  `json:"intValue,omitempty"`
	DoubleValue *float64 `json:"doubleValue,omitempty"`
	BoolValue   *bool    `json:"boolValue,omitempty"`
}

func buildOTLPJSON(spans []sdktrace.ReadOnlySpan) otlpExportRequest {
	// Group by resource
	type resGroup struct {
		res    sdktrace.ReadOnlySpan
		scopes map[string][]sdktrace.ReadOnlySpan
	}
	byRes := make(map[string]*resGroup)
	var resOrder []string

	for _, s := range spans {
		resKey := spanResKey(s)
		if _, ok := byRes[resKey]; !ok {
			byRes[resKey] = &resGroup{res: s, scopes: map[string][]sdktrace.ReadOnlySpan{}}
			resOrder = append(resOrder, resKey)
		}
		scopeKey := s.InstrumentationScope().Name
		byRes[resKey].scopes[scopeKey] = append(byRes[resKey].scopes[scopeKey], s)
	}

	var resourceSpans []otlpResourceSpans
	for _, rk := range resOrder {
		g := byRes[rk]
		rs := otlpResourceSpans{
			Resource: encodeResource(g.res),
		}
		for scopeName, ss := range g.scopes {
			scope := g.res.InstrumentationScope()
			sc := otlpScopeSpans{
				Scope: otlpScope{Name: scopeName, Version: scope.Version},
			}
			for _, s := range ss {
				sc.Spans = append(sc.Spans, encodeSpan(s))
			}
			rs.ScopeSpans = append(rs.ScopeSpans, sc)
		}
		resourceSpans = append(resourceSpans, rs)
	}
	return otlpExportRequest{ResourceSpans: resourceSpans}
}

func spanResKey(s sdktrace.ReadOnlySpan) string {
	attrs := s.Resource().Attributes()
	b, _ := json.Marshal(attrs)
	return string(b)
}

func encodeResource(s sdktrace.ReadOnlySpan) otlpResource {
	return otlpResource{Attributes: encodeAttrs(s.Resource().Attributes())}
}

func encodeSpan(s sdktrace.ReadOnlySpan) otlpSpan {
	ctx := s.SpanContext()
	sp := otlpSpan{
		TraceID:           ctx.TraceID().String(),
		SpanID:            ctx.SpanID().String(),
		Name:              s.Name(),
		Kind:              int(s.SpanKind()),
		StartTimeUnixNano: strconv.FormatInt(s.StartTime().UnixNano(), 10),
		EndTimeUnixNano:   strconv.FormatInt(s.EndTime().UnixNano(), 10),
		Attributes:        encodeAttrs(s.Attributes()),
		Status:            encodeStatus(s),
	}
	if parent := s.Parent(); parent.IsValid() {
		sp.ParentSpanID = parent.SpanID().String()
	}
	for _, ev := range s.Events() {
		sp.Events = append(sp.Events, otlpEvent{
			TimeUnixNano: strconv.FormatInt(ev.Time.UnixNano(), 10),
			Name:         ev.Name,
			Attributes:   encodeAttrs(ev.Attributes),
		})
	}
	return sp
}

func encodeStatus(s sdktrace.ReadOnlySpan) otlpStatus {
	st := s.Status()
	code := 0
	switch st.Code {
	case sdktrace.Status{}.Code: // UNSET
		code = 0
	}
	// Use numeric comparison to avoid importing codes package
	// sdktrace.Status.Code is 0=Unset, 1=Error, 2=Ok in the Go SDK
	return otlpStatus{Code: code, Message: st.Description}
}

func encodeAttrs(attrs []attribute.KeyValue) []otlpKV {
	kvs := make([]otlpKV, 0, len(attrs))
	for _, a := range attrs {
		kv := otlpKV{Key: string(a.Key)}
		switch a.Value.Type() {
		case attribute.STRING:
			v := a.Value.AsString()
			kv.Value = otlpValue{StringValue: &v}
		case attribute.INT64:
			v := strconv.FormatInt(a.Value.AsInt64(), 10)
			kv.Value = otlpValue{IntValue: &v}
		case attribute.FLOAT64:
			v := a.Value.AsFloat64()
			kv.Value = otlpValue{DoubleValue: &v}
		case attribute.BOOL:
			v := a.Value.AsBool()
			kv.Value = otlpValue{BoolValue: &v}
		default:
			v := a.Value.Emit()
			kv.Value = otlpValue{StringValue: &v}
		}
		kvs = append(kvs, kv)
	}
	return kvs
}

// ---------------------------------------------------------------------------
// SetupOTEL — initialise the global tracer provider
// ---------------------------------------------------------------------------

// OTELShutdown flushes and closes the exporter. Defer it in main().
type OTELShutdown func(ctx context.Context) error

// SetupOTEL initialises a global TracerProvider writing OTLP/JSON traces to
// <logsDir>/<serviceName>-traces-YYYY-MM-DD.jsonl.
// logsDir defaults to "logs" if empty.
func SetupOTEL(serviceName, logsDir string) (OTELShutdown, error) {
	if logsDir == "" {
		logsDir = "logs"
	}
	exp, err := newOTLPJsonExporter(serviceName, logsDir)
	if err != nil {
		return nil, err
	}
	res, err := sdkresource.New(context.Background(),
		sdkresource.WithAttributes(
			semconv.ServiceName(serviceName),
			semconv.TelemetrySDKLanguageGo,
		),
	)
	if err != nil {
		res = sdkresource.Default()
	}
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithBatcher(exp),
		sdktrace.WithResource(res),
		sdktrace.WithSampler(sdktrace.AlwaysSample()),
	)
	otel.SetTracerProvider(tp)
	return func(ctx context.Context) error {
		return tp.Shutdown(ctx)
	}, nil
}

// ---------------------------------------------------------------------------
// StartToolSpan — child span from Python-propagated trace context
// ---------------------------------------------------------------------------

// StartToolSpan creates an OTEL child span for a tool.call, parenting it under
// the trace context propagated from the Python control plane via MCP-lite fields.
// Returns the span and a finish function — call finish(err) when the tool exits.
//
//	span, finish := mcplite.StartToolSpan(req, "whatsapp")
//	defer finish(toolErr)
func StartToolSpan(req ToolCallRequest, serviceName string) (oteltrace.Span, func(error)) {
	tr := otel.Tracer(serviceName)
	ctx := context.Background()

	if req.TraceID != "" && req.SpanID != "" {
		if tid, sid, ok := parseIDs(req.TraceID, req.SpanID); ok {
			sc := oteltrace.NewSpanContext(oteltrace.SpanContextConfig{
				TraceID:    tid,
				SpanID:     sid,
				TraceFlags: oteltrace.FlagsSampled,
				Remote:     true,
			})
			ctx = oteltrace.ContextWithRemoteSpanContext(ctx, sc)
		}
	}

	_, span := tr.Start(ctx, "tool/"+req.Tool, oteltrace.WithAttributes(
		attribute.String("tool", req.Tool),
		attribute.String("service", serviceName),
		attribute.String("request_id", req.ID),
	))
	finish := func(err error) {
		if err != nil {
			span.RecordError(err)
		}
		span.End()
	}
	return span, finish
}

func parseIDs(traceHex, spanHex string) (oteltrace.TraceID, oteltrace.SpanID, bool) {
	tb, terr := hex.DecodeString(traceHex)
	sb, serr := hex.DecodeString(spanHex)
	if terr != nil || serr != nil || len(tb) != 16 || len(sb) != 8 {
		return oteltrace.TraceID{}, oteltrace.SpanID{}, false
	}
	var tid oteltrace.TraceID
	var sid oteltrace.SpanID
	copy(tid[:], tb)
	copy(sid[:], sb)
	return tid, sid, true
}

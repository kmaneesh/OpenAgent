module github.com/kmaneesh/openagent/services/slack

go 1.22.0

require (
	github.com/kmaneesh/openagent/services/sdk-go v0.0.0
	github.com/slack-go/slack v0.17.3
)

require (
	github.com/go-logr/logr v1.4.2 // indirect
	github.com/go-logr/stdr v1.2.2 // indirect
	github.com/google/uuid v1.6.0 // indirect
	github.com/gorilla/websocket v1.5.3 // indirect
	go.opentelemetry.io/auto/sdk v1.1.0 // indirect
	go.opentelemetry.io/otel v1.35.0 // indirect
	go.opentelemetry.io/otel/metric v1.35.0 // indirect
	go.opentelemetry.io/otel/sdk v1.35.0 // indirect
	go.opentelemetry.io/otel/trace v1.35.0 // indirect
	golang.org/x/sys v0.30.0 // indirect
)

replace github.com/kmaneesh/openagent/services/sdk-go => ../sdk-go

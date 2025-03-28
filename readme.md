# K8s Port Forward

K8s Port Forward is a command-line tool that simplifies Kubernetes port-forwarding with enhanced logging, configurable timeouts, and liveness probing. This README provides information on how to use this tool, explanations of its command-line arguments, configuration file examples, and details about verbosity levels.

## Features

- **Port-forwarding**: Forward a specified Kubernetes resource to a local port.
- **Configurable timeouts**: Set connection timeouts via the command line or configuration file.
- **Liveness probing**: Monitor the health of the port-forwarding connection using an HTTP endpoint.
- **Detailed logging**: Choose verbosity levels for debugging and operational insight.

## Installation

Clone the repository and build the project with Cargo:

```bash
cargo build --release
```

## Usage

You can run the tool either in single resource mode or using a configuration file for multiple resources.

### Single Resource Mode

Specify the Kubernetes resource directly on the command line. The resource format is:

```
type/name:port
```

For example:
- `pod/my-pod:8080`
- `service/my-service:80`

Example command:

```bash
./k8s-port-forward pod/my-pod:8080 --local_port 9090 --verbose 2 --timeout 5 --liveness_probe /ping --show_liveness
```

### Configuration File Mode

Create a JSON configuration file (e.g., `config.json`) with the following structure:

```json
{
  "forwards": [
    {
      "resource": "service/nes-pn:80",
      "local_port": 55400,
      "timeout": 5,
      "liveness_probe": "/ping"
    },
    {
      "resource": "service/api-auth:80",
      "local_port": 55300,
      "timeout": 5,
      "liveness_probe": "/ping"
    }
  ],
  "verbose": 1
}
```

Then run the tool with:

```bash
./k8s-port-forward --config config.json --verbose 3 --timeout 10
```

### Command-Line Arguments

- `--resource <RESOURCE>`: Specify a single Kubernetes resource to port-forward (format: type/name:port).
- `--local_port <PORT>`: Local port to listen on when using a single resource.
- `--config <CONFIG>`: Path to a JSON configuration file containing multiple port-forwards.
- `--verbose <VERBOSE>`: Verbosity level (0-3). Higher values produce more detailed logs.
  - **Level 0**: No logging output.
  - **Level 1**: Basic logging and status updates.
  - **Level 2**: Additional logging, including request bodies (except for GET requests).
  - **Level 3**: Detailed logging with response body inspection and JSON syntax highlighting.
- `--timeout <TIMEOUT>`: Timeout in seconds for the port-forward connection.
- `--liveness_probe <PATH>`: HTTP endpoint path used for health checks (e.g., `/ping`).
- `--show_liveness`: Flag to enable logging for liveness probe requests (disabled by default).
- `--requests_log_file <FILE>`: Path to a log file for writing detailed requests/responses. Output is in plain text without ANSI color codes, and JSON payloads are serialized as one line.
- `--requests_log_verbosity <VERBOSE>`: Verbosity level for file logging (0-3). Higher values include additional details, such as full request/response payloads.

### Verbosity Levels Explained

- **Verbose 1**: Show essential information about port-forwarding, including start-ups, connection attempts, and failure messages.
- **Verbose 2**: In addition to level 1, log request bodies (except for GET requests) to help diagnose issues.
- **Verbose 3**: Provide detailed logging with response body content, including syntax-highlighted JSON.
- **Verbose 4**: Display maximum level debug logs, including all available diagnostic information.

## Internal Endpoints

The tool exposes an internal endpoint to check port-forward health:

- `/_internal/status`: Returns a JSON payload with health details, including:
  - `active`: Whether the port-forward is currently active.
  - `last_ping`: Timestamp of the last health check.
  - `latency`: The current latency of the connection (currently reported as "unknown").
  
## Example Log Output

Successful request log example:

```
✓ service/nes-pn:80 - GET /internal/babies/03bf072b/users → 200 (150ms)
```

Error log example:

```
✗ service/nes-pn:80 - GET /internal/babies/03bf072b/users → 502 Bad Gateway (300ms)
```

## Troubleshooting

- Ensure your Kubernetes credentials are set up correctly (e.g., via `kubectl`).
- Adjust verbosity using the `--verbose` option to obtain more diagnostic information.
- Check the internal status endpoint (`/_internal/status`) for real-time health and connection feedback.

## License

MIT License

## Author

Your Name <your.email@example.com>

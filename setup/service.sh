# Install config file and systemd service.
# Sourced by setup.sh — expects BIN_DIR, CONFIG_DIR, DATA_DIR, etc.

install_config() {
    local config_file="${CONFIG_DIR}/config.yaml"

    if [[ -f "$config_file" ]]; then
        info "Config exists at ${config_file}, not overwriting"
        return 0
    fi

    info "Writing config to ${config_file}..."
    cat > "$config_file" << EOF
listen: "0.0.0.0:7000"
data_dir: "${DATA_DIR}"
log_level: "info"

firecracker:
  binary: "${BIN_DIR}/firecracker"
  jailer: "${BIN_DIR}/jailer"
  default_kernel: "vmlinux-6.1"
  default_vcpus: 2
  default_memory_mib: 512

networking:
  bridge: "zerobox-br0"
  subnet: "10.20.0.0/16"
  host_port_range: "30000-40000"
  outbound_interface: "auto"

timeouts:
  default_sandbox_timeout_ms: 300000
  max_sandbox_timeout_ms: 18000000

snapshots:
  storage_dir: "${SNAPSHOTS_DIR}"
  max_snapshots_per_sandbox: 50
  auto_snapshot_interval_ms: 0

images:
  cache_dir: "${IMAGES_DIR}"

auth:
  enabled: false
  tokens: []
EOF

    chmod 640 "$config_file"
    chown "root:${ZEROBOX_USER}" "$config_file"
}

install_systemd_service() {
    if ! command_exists systemctl; then
        warn "systemd not found. Run the daemon manually:"
        printf "    %s serve --config %s/config.yaml\n" "${BIN_DIR}/zerobox" "$CONFIG_DIR"
        return 0
    fi

    info "Installing systemd service..."
    cat > /etc/systemd/system/zerobox.service << EOF
[Unit]
Description=Zerobox - Firecracker Sandbox Supervisor
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
ExecStart=${BIN_DIR}/zerobox serve --config ${CONFIG_DIR}/config.yaml
Restart=on-failure
RestartSec=5s
StartLimitIntervalSec=300
StartLimitBurst=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=zerobox
LimitNOFILE=65536
LimitNPROC=4096
ProtectHome=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    info "Installed systemd service"
}

start_service() {
    if ! command_exists systemctl; then
        return 0
    fi

    systemctl enable zerobox.service >/dev/null 2>&1

    if systemctl is-active --quiet zerobox.service; then
        info "Restarting zerobox..."
        systemctl restart zerobox.service
    else
        info "Starting zerobox..."
        systemctl start zerobox.service
    fi

    sleep 1

    if systemctl is-active --quiet zerobox.service; then
        info "zerobox is running"
    else
        warn "zerobox failed to start. Check: journalctl -u zerobox -n 50"
    fi
}

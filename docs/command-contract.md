# Command Contract

All frontends communicate through the same protocol-agnostic command contract.

## Command JSON Shape

Commands are tagged with `command`:

```json
{
  "command": "connect",
  "profile": {
    "protocol": "sftp",
    "host": "example.com",
    "port": 22,
    "username": "alice",
    "password": "secret",
    "private_key_pem": null,
    "private_key_path": "~/.ssh/id_rsa",
    "security": "ssh_transport",
    "strict_host_key_checking": true,
    "passive_mode": true
  }
}
```

Supported command tags:

- `connect`
- `disconnect`
- `list_directory`
- `upload_file`
- `download_file`
- `delete_path`
- `rename_path`
- `supported_protocols`

## Response JSON Shape

Responses are tagged with `status`:

- `connected`
- `disconnected`
- `directory`
- `transfer_completed`
- `path_deleted`
- `path_renamed`
- `supported_protocols`
- `ok`
- `error`

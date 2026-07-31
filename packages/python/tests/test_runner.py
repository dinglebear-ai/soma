import os
import socket
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from soma_runner_protocol import decode_frame, encode_frame


class PersistentRunnerTests(unittest.TestCase):
    def test_invocation_uses_authenticated_host_call_round_trip(self):
        with tempfile.TemporaryDirectory() as directory:
            provider = Path(directory) / "broker.py"
            provider.write_text(
                "from soma_provider import Context, provider, tool\n"
                "PROVIDER = provider(name='broker-test', kind='python')\n"
                "@tool\n"
                "async def broker_check(ctx: Context):\n"
                "    return {'cancelled': await ctx.cancellation.is_cancelled()}\n",
                encoding="utf-8",
            )
            parent, child = socket.socketpair()
            env = dict(os.environ, SOMA_PYTHON_RUNNER_FD=str(child.fileno()))
            process = subprocess.Popen(
                [sys.executable, "-m", "soma_provider.runner"],
                env=env,
                pass_fds=[child.fileno()],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            child.close()
            try:
                self._read_socket_frame(parent)
                parent.sendall(encode_frame({
                    "type": "initialize",
                    "protocol": {"major": 1, "minor": 0},
                    "features": ["describe", "invoke", "host-calls", "shutdown"],
                    "generation_id": "broker-generation",
                }))
                self.assertEqual(self._read_socket_frame(parent)["type"], "ready")
                parent.sendall(encode_frame({
                    "type": "request",
                    "method": "describe",
                    "request_id": 1,
                    "path": str(provider),
                    "generation_id": "broker-generation",
                }))
                self.assertEqual(self._read_socket_frame(parent)["status"], "ok")
                parent.sendall(encode_frame({
                    "type": "request",
                    "method": "invoke",
                    "request_id": 2,
                    "invocation": {
                        "invocation_id": "invocation-2",
                        "provider": "broker-test",
                        "action": "broker_check",
                        "arguments": {},
                        "surface": "mcp",
                        "snapshot_id": "snapshot",
                        "deadline_unix_ms": 1900000000000,
                        "actor": {"actor_id": "alice", "scopes": ["soma:read"]},
                        "cancellation_token_id": "cancel-2",
                        "generation_id": "broker-generation",
                    },
                }))
                self.assertEqual(self._read_socket_frame(parent)["status"], "accepted")
                host_call = self._read_socket_frame(parent)
                self.assertEqual(host_call.get("method"), "host.cancelled", host_call)
                self.assertEqual(host_call["invocation_id"], "invocation-2")
                parent.sendall(encode_frame({
                    "type": "host_reply",
                    "request_id": host_call["request_id"],
                    "result": False,
                }))
                result = self._read_socket_frame(parent)
                self.assertEqual(result["result"], {"cancelled": False})
                parent.sendall(encode_frame({
                    "type": "request", "method": "shutdown", "request_id": 3
                }))
                self.assertEqual(self._read_socket_frame(parent)["status"], "ok")
                self.assertEqual(process.wait(timeout=3), 0)
            finally:
                parent.close()
                if process.poll() is None:
                    process.kill()
                    process.wait()
                process.stdout.close()
                process.stderr.close()

    def test_isolated_module_supports_explicit_test_stdio_control_channel(self):
        env = dict(os.environ, SOMA_PYTHON_RUNNER_TEST_STDIO="1")
        source_root = Path(__file__).resolve().parents[1] / "python"
        bootstrap = (
            "import runpy,sys;"
            f"sys.path.insert(0,{str(source_root)!r});"
            "runpy.run_module('soma_provider.runner',run_name='__main__')"
        )
        process = subprocess.Popen(
            [sys.executable, "-I", "-c", bootstrap],
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            hello = self._read_pipe_frame(process.stdout)
            self.assertEqual(hello["type"], "hello")
            process.stdin.write(encode_frame({
                "type": "initialize",
                "protocol": {"major": 1, "minor": 0},
                "features": ["health", "shutdown"],
                "generation_id": "stdio-test",
            }))
            process.stdin.flush()
            ready = self._read_pipe_frame(process.stdout)
            self.assertEqual(ready["generation_id"], "stdio-test")
            process.stdin.write(encode_frame({
                "type": "request", "method": "health", "request_id": 1
            }))
            process.stdin.flush()
            health = self._read_pipe_frame(process.stdout)
            self.assertEqual(health["generation_id"], "stdio-test")
            process.stdin.write(encode_frame({
                "type": "request", "method": "shutdown", "request_id": 2
            }))
            process.stdin.flush()
            self.assertEqual(self._read_pipe_frame(process.stdout)["status"], "ok")
            self.assertEqual(process.wait(timeout=3), 0)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stdin.close()
            process.stdout.close()
            process.stderr.close()

    def test_hello_initialize_health_and_shutdown_use_private_descriptor(self):
        parent, child = socket.socketpair()
        env = dict(os.environ, SOMA_PYTHON_RUNNER_FD=str(child.fileno()))
        process = subprocess.Popen(
            [sys.executable, "-m", "soma_provider.runner"],
            env=env,
            pass_fds=[child.fileno()],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        child.close()
        try:
            header = parent.recv(4)
            size = int.from_bytes(header, "big")
            hello = decode_frame(header + parent.recv(size))
            self.assertEqual(hello["type"], "hello")
            parent.sendall(encode_frame({
                "type": "initialize",
                "protocol": {"major": 1, "minor": 0},
                "features": ["health"],
                "generation_id": "socket-test",
            }))
            header = parent.recv(4)
            ready = decode_frame(header + parent.recv(int.from_bytes(header, "big")))
            self.assertEqual(ready["type"], "ready")
            parent.sendall(encode_frame({"type": "request", "method": "health", "request_id": 1}))
            header = parent.recv(4)
            reply = decode_frame(header + parent.recv(int.from_bytes(header, "big")))
            self.assertEqual(reply["health"], "ready")
            parent.sendall(encode_frame({"type": "request", "method": "shutdown", "request_id": 2}))
            process.wait(timeout=3)
            self.assertEqual(process.stdout.read(), b"")
        finally:
            parent.close()
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stdout.close()
            process.stderr.close()

    @staticmethod
    def _read_pipe_frame(pipe):
        header = pipe.read(4)
        if len(header) != 4:
            raise AssertionError("runner closed before frame header")
        payload = pipe.read(int.from_bytes(header, "big"))
        return decode_frame(header + payload)

    @staticmethod
    def _read_socket_frame(channel):
        header = channel.recv(4)
        if len(header) != 4:
            raise AssertionError("runner closed before frame header")
        remaining = int.from_bytes(header, "big")
        chunks = bytearray()
        while len(chunks) < remaining:
            chunk = channel.recv(remaining - len(chunks))
            if not chunk:
                raise AssertionError("runner closed during frame")
            chunks.extend(chunk)
        return decode_frame(header + bytes(chunks))

if __name__ == "__main__":
    unittest.main()

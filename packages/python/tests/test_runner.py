import os
import socket
import subprocess
import sys
import unittest

from soma_runner_protocol import decode_frame, encode_frame


class PersistentRunnerTests(unittest.TestCase):
    def test_isolated_module_uses_reserved_stdio_control_channel(self):
        process = subprocess.Popen(
            [sys.executable, "-I", "-m", "soma_provider.runner"],
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

if __name__ == "__main__":
    unittest.main()

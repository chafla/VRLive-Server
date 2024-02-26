# echo-client.py

import socket
import json
import sys
import threading
import time
from os import path

HOST = "localhost"  # The server's hostname or IP address
PORT = 5653  # The port used by the server

try:
    port_offset = int(sys.argv[1])
except (ValueError, IndexError):
    print("Defaulting to zero port offset")
    port_offset = 0

synack_resp = {
    "user_type": 2,
    "user_id": 0,
    "own_identifier": "python server",
    "user_flags": [],
    "backing_track_port": 6100 + port_offset,
    "server_event_port": 6101 + port_offset,
    "audience_motion_capture_port": 6102 + port_offset,
    "extra_ports": {},
    "vrtp_mocap_port": 6103 + port_offset
}

remote_host = None

def handshake():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((HOST, PORT))
        # s.sendall(b"Hello, world")
        data = s.recv(1024)
        print(f"Received {data!r}")
        raw_in = json.loads(data.decode("utf-8"))
        synack_resp["user_id"] = raw_in["user_id"]
        s.send(json.dumps(synack_resp).encode("utf-8"))
        data = s.recv(1024)
        print(f"Received {data!r}")


def server_event_thread():
    # prep to send events
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((HOST, 5657))
        # s.accept()
        # s.listen()
        # conn, addr = s.accept()
        # with conn:
        #     while True:
        while True:
            data = s.recv(300)
            if not data:
                print("EOF")
                sys.exit(0)
                break
            print(data)
    # s.connect((HOST, PORT))

def client_event_thread():
    # prep to send events
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((HOST, 5655))
        s.send(b"aaaaaaaaaaaaaa")
        # s.accept()
        # s.listen()
        # conn, addr = s.accept()
        # with conn:
        while True:
            pass
        #     while True:
        #         data = conn.recv(300)
        #         print(data)

def backing_track_thread():
    # prep to send events
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((HOST, 5656))
        # s.send(b"aaaaaaaaaaaaaa")
        # s.accept()
        # s.listen()
        # conn, addr = s.accept()
        # with conn:
        while True:
            data = s.recv(8)
            if data == b'NEWTRACK':
                header_len = int.from_bytes(s.recv(2), "big")
                body_len = int.from_bytes(s.recv(4), "big")
                print("new track incoming!")
                # payload = s.recv(8)
                title_length = s.recv(2)
                filename = s.recv(int.from_bytes(title_length, "big"))
                # length =
                print(f"new track: {filename.decode('utf-8')} ({body_len} bytes)")
                # actual_data = s.recv(int(body_len))
                p = path.basename(filename)
                with open(p, "wb") as f:
                    f.write(s.recv(body_len))

            if not data:
                print("Backing track EOF")
                break
            print("Backing track")
            print(data.decode("utf-8"))

def main():
    handshake()
    # time.sleep(1)
    threading.Thread(target=server_event_thread).start()
    threading.Thread(target=client_event_thread).start()
    threading.Thread(target=backing_track_thread()).start()

if __name__ == '__main__':
    main()
    while True:
        time.sleep(5)


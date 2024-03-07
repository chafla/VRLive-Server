# echo-client.py
import queue
import socket
import json
import sys
import threading
import time
from os import path
import pythonosc.osc_packet
from pythonosc import parsing
from osc4py3.oscbuildparse import decode_packet


performer_mocap_queue = queue.Queue()

HOST = "localhost"  # The server's hostname or IP address
# PORT = 5653  # The port used by the server
OSC_IN_HOST = "129.21.149.239"

remote_ports = {
    "new_connection": 5653,
    "performer_mocap": 5654,
    "performer_audio": 5655,
    "client_event": 5656,
    "backing_track_sock": 5657,
    "server_event_sock": 5658,
    "audience_mocap": 9000
}

osc_out = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
osc_out.connect((OSC_IN_HOST, 9050))

try:
    port_offset = int(sys.argv[1])
except (ValueError, IndexError):
    print("Defaulting to zero port offset")
    port_offset = 0

ports = {
    "backing_track": 6100 + port_offset,
    "server_event": 6101 + port_offset,
    "audience_motion_capture": 6102 + port_offset,
    "extra_ports": {},
    "vrtp_data": 6103 + port_offset
}

synack_resp = {
    "user_type": 2,
    "user_id": 0,
    "own_identifier": "python server",
    "user_flags": [],
    "ports": ports

}

remote_host = None

def handshake():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((HOST, remote_ports["new_connection"]))
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
        s.connect((HOST, remote_ports["server_event_sock"]))
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
            print(">> Server message:", data.decode("utf-8"))
    # s.connect((HOST, PORT))

def client_event_thread():
    # prep to send events
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((HOST, remote_ports["client_event"]))
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


def vrtp_in_thread():

    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
        s.bind((HOST, ports["vrtp_data"]))
        print("Bound for vrtp")
        audio_file = open("audio_out.ogg", "wb")
        mocap_file = open("mocap_out.osc", "wb")
        while True:
            data = s.recv(50000)
            deconstruct_vrtp(data, audio_file, mocap_file, osc_out)

            # print("VRTP data in")

def mocap_in_thread():
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
        s.bind((HOST, ports["audience_motion_capture"]))
        while True:
            data = s.recv(2048)
            osc_out.send(data)
            print("audience mocap data in")

packets_in = 0

def deconstruct_vrtp(pkt_in: bytes, audio_file, mocap_file, mocap_sock):
    total_pl_size = int.from_bytes(pkt_in[0:4], "big")
    osc_size = int.from_bytes(pkt_in[4:6], "big")
    audio_size = int.from_bytes(pkt_in[6:8], "big")
    osc_data = pkt_in[8:osc_size + 8]
    audio_data = pkt_in[8+osc_size:]
    # decoder = pyogg.opus.OpusDecoder()
    # res = pyogg.opus.opus_decode(decoder, audio_data, audio_size, )
    audio_file.write(audio_data)
    mocap_file.write(osc_data)
    if osc_data:
        # list comps my beloved
        res = decode_packet(osc_data)
        global packets_in  # hehe
        packets_in += 1
        if packets_in % 100 == 0:
            # just to make sure it works
            pkt = pythonosc.osc_packet.OscPacket(osc_data)
        performer_mocap_queue.put(osc_data)
        # [performer_mocap_queue.put(b"#bundle" + b) for b in osc_data.split(b"#bundle")[1:]]
    # (osc_data)
    # mocap_sock.send(osc_data)
    # print("a")

def vrtp_mocap_relay():
    while True:
        mocap_data = performer_mocap_queue.get()
        osc_out.send(mocap_data)
        # data = decode_packet(mocap_data)
        # print("guh")



def backing_track_thread():
    # prep to send events
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((HOST, remote_ports["backing_track_sock"]))
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
    threading.Thread(target=backing_track_thread).start()
    threading.Thread(target=mocap_in_thread).start()
    threading.Thread(target=vrtp_in_thread).start()
    threading.Thread(target=vrtp_mocap_relay).start()


if __name__ == '__main__':
    main()
    while True:
        time.sleep(5)


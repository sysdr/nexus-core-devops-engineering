#!/usr/bin/env python3
import socket, sys

HOST = "127.0.0.1"
PORT = 9090

srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind((HOST, PORT))
srv.listen(128)
sys.stderr.write(f"[sink] Listening on {HOST}:{PORT}\n")
sys.stderr.flush()

while True:
    conn, _ = srv.accept()
    try:
        while conn.recv(4096):
            pass
    finally:
        conn.close()

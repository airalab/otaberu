import socket
import os
import json
import pprint
import time
import sys

socket_path = 'm1.socket'

def send_request(action, content=None):
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.connect(os.path.realpath(socket_path))
    message = {"action": action}
    if content:
        message["content"] = content
    client.sendall(json.dumps(message).encode())
    response = client.recv(2048).decode()
    client.close()
    print(response)
    return json.loads(response)

def subscribe_messages():
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.connect(os.path.realpath(socket_path))
            print("Subscribed to messages")
            client.sendall(json.dumps({"action": "/subscribe_messages"}).encode())
            while True:
                time.sleep(0.05)
                response = client.recv(1024).decode()
                if response:
                    print(response)
                    

def main():
    pass
    #robot = send_request("/me")
    #print('=== /me ===')
    #pprint.pprint(robot)
    local_devices = send_request("/local_robots")
    print('=== /local_robots ===')
    pprint.pprint(local_devices)
    exit(0)
    #subscribe_messages()
    #while True:
    #    inp = sys.stdin.read(1)
    #    print(inp)
    #    #res = send_request("/send_message", "w")
    #    #print(res)
def press(key):
    print(key)
    res =send_request("/send_message", key)
    print(res)

if __name__=='__main__':
    main()
    subscribe_messages()

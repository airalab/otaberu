# Merklebot's Robot-Agent

## Roadmap

- [X] auto restart and system service launch
- [ ] config on the platform 
- [ ] tui for agent state monitoring
- [ ] key based auth
- [ ] autonomous config update

## Installation and run
### You can run agent directly from terminal
1. Manually [download latest release](https://github.com/merklebot/robot-agent/releases) of agent for your system
2. Make agent executable
```bash
chmod +x robot-agent-...
```
3. Run agent with ``api-key`` (run only with -k parameter if do not need platform)

```bash
./robot-agent-... -a <API_KEY> -r http://robots.merklebot.com:8888 -k <BASE_64_OF_ED25519>
```

### Run agent as systemd service
Use easy to run script:
```bash
curl -s https://app.merklebot.com/install.sh | bash -s <API_KEY>
```
The copy of script could be found at [this repository](https://github.com/merklebot/robot-agent/blob/main/install.sh)

To stop agent
```bash
systemctl disable merklebot 
```

## Modules
The agent consists of 4 main modules:
- Platform module
- Docker module
- Libp2p interaction module
- Unix Socket Interface


![Agent Scheme](docs/assets/Agent%20Scheme.png)

### Main (Platform)
First of all, robot-agent can be connected to merklebot's platform (app.merklebot.com) to get jobs from the cloud. 
For authentication, agent needs `api-key` which is available on a platform after your robot instance is created. 

> **Note**
> In next version of agent `api-key` will be changed for `key` (check libp2p section)
<!-- (WILL USE ONLY `key` IN THE FUTURE) --> 

### Docker
All the jobs sent from platform are splitted to 2 categories:

1. Execution of container wrapper code
2. Direct access to the terminal (of containerized environment)

Via port forwarding and docker volumes system it could be extended to manipulation of the whole host system.

It is also possible to store some data after job execution in bucket on merklebot's platform.

### Libp2p

Libp2p protocol is used for advanced discovery of other agents (via mDNS) and messaging between agents.

For usage of this module, you'll need a `key` - base64 encoded key.

<!-- Discovering other agents in the network and message forwarding.
It needs `key` - base64 encoded private key. -->

### Unix Socket Interface
Interface for applications and containers on devices. It creates a `merklebot.socket` file which provides JSON API for interactions with an agent. 


## Feautures 

### Docker Job running

With [platform](app.merklbot.com) or via [API](https://github.com/merklebot/mb-cli) some code wrapped in docker container could be executed as job.


![Docker Job Screen](docs/assets/Docker%20Job.png)

1. **Docker Image**

Insert link for a container wrapped code. It should be stored in some publicly accessible container register (in other words `docker pull LINK` should work) 

2. Docker Container Name

Name container to your liking, but with standard docker naming limitations

3. Environmental variables

Add all the variables you think you'll need (like API keys, super secrets and other cool things)

4. Network mode:
Switch between closed `default` network or allow `host` one

5. Privileged mode

Run container with admin privileges. It could be used, if devices or specific host system volumes are needed in use.

6. Port binding

Share ports between container and host system

7. Volumes

Share folders or devices between container and host system

#### Store data on the platform

To store data after job is executed:

1) Create bucket at app.merklebot.com
2) Assign the bucket to the robot
3) Turn on `Store data` in job configuration
4) Place required files in `/merklebot/job_data/` folder

After the code execution, files would be loaded to your bucket and accessible in job result screen

#### Access container's terminal

To access terminal of container via web-interface, add `sh` to your `custom tty` filed of job config.

The terminal could be reached in job status screen (OPEN TERMINAL). 

>**Note**
>To end job, don't forget to send `exit`.


### Devices discovery

To connect different to devices (with different agents on them), you should create a device group config and spread it around devices. With the usage of this config, agent can match PeerId's and return IP addresses of devices in local network.

It could be used to avoid static IP for your group of devices.

To get devices, send JSON  `{"action": "/local_robots"}` to  `merklebot.socket`

Example of `config.json`:
```JSON
{
  "robots": [
    {
      "robot_id": "device-0",
      "robot_peer_id": "12D3KooWKnY2J5CFny3Ef8abndmg9U4gAkndUDPM4oMP7yVbftBK",
      "name": "spot",
      "tags": [],
      "interfaces": []
    },
    {
      "robot_id": "device-1",
      "robot_peer_id": "12D3KooWK8ogtRq7DXD21ji9nwkTgCfuz6AYU9dys8GrH7weC2AC",
      "name": "jetson",
      "tags": [],
      "interfaces": []
    }
  ]
}
```

Example of python code:
```python
import socket
import os
import json
import pprint

socket_path = "FOLDER_WITH_AGENT/merklebot.socket"

def local_robots():
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.connect(os.path.realpath(socket_path))
    message = {"action": "/local_robots"}
    client.sendall(json.dumps(message).encode())
    response = client.recv(1024).decode()
    client.close()
    return json.loads(response)

pprint.pprint(local_robots()) # prints json with found devices
```

 
### Libp2p messaging 

If sending some text messages is more than enough for your group of devices, you could use internal massages of agent based on libp2p messaging. To use it, you'll need a robot group config like in previous section.

To send message, send JSON `{"action": "/send_message", "content": "TEXT_OF_MESSAGE"}` to  `merklebot.socket`

To receive one, subscribe with JSON `{"action": "/subscribe_messages"}`

Python code example:
```python
import socket
import os
import json
import pprint
import time

socket_path = "FOLDER_WITH_AGENT/merklebot.socket"

def send_message(message_content):
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.connect(os.path.realpath(socket_path))
    message = {"action": "/send_message"}
    message["content"] = message_content
    client.sendall(json.dumps(message).encode())
    response = client.recv(1024).decode()
    client.close()
    return json.loads(response)

def subscribe_messages():
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(os.path.realpath(socket_path))
        client.sendall(json.dumps({"action": "/subscribe_messages"}).encode())
        while True:
            time.sleep(0.1)
            response = client.recv(1024).decode()
            if response:
                print(response)

```





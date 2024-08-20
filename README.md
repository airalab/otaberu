## About Robot Agent
Robot Agent enables device-to-device communication based on [libp2p protocol](https://libp2p.io). With Robot Agent, you can organize geographically dispersed robotic operations easily. The software facilitates setup of CI/CD and data management pipelines, all while ensuring secure networking.

Currently, it supports sending Docker jobs to devices via libp2p and allows direct connection to these jobs through a CLI terminal. CLI tool with terminal UI is a convenient way to manage the robots in your fleet: logs, media data, Docker containers and configs. 


## Download agent
Download agent from https://github.com/otaberu/robot-agent/releases/latest for your platform architecture

*For example:*
`wget https://github.com/otaberu/robot-agent/releases/download/0.1.1/robot-agent-aarch64-apple-darwin`

Rename it 
`mv robot-agent-aarch64-apple-darwin robot-agent`

And make executable:
`chmod +x robot-agent`

## Install rn-cli
`pip3 install rn-cli --upgrade`

## Create owner key
We create a keypair for an organization owner.  Public key will be place to all agents while startup. Secret key is used to sign messages for the Robot Network.

To create key, use cli command:
`rn keys gen owner.key`

After running this command, key will be created by set path and you will see public key in `base64` format:
`Public Key: Qwu4TtfNOcQzMJkGiYvJ4IuZSuszM0w1ViEEuAHlzo0=`

To continue, let's set `USER_KEY_PATH` environment variable that contains path to owner key
`export USER_KEY_PATH=owner.key`
## Create configuration for robots network
==To publish config, you should firstly run agent on your machine and set path to socket(same folder as an agent by default).==
==`export AGENT_SOCKET_PATH=rn.socket`==

We use cli's TUI to configure a list of robots:
`rn tui config robots.json`

After launch, you will see an interface with robots:
![](https://i.ibb.co/PQfm3zy/Pasted-image-20240812210939.png)

Then we click "Add robot" to generate key and set info:
![](https://i.ibb.co/Ykv83KF/Pasted-image-20240812211358.png)


**Save Private Key, it will be used to start agent on a device. If you have problem to select a text, try holding shift or option key**

To publish config, you should firstly run agent(add new robot without publishing yet) on your machine and set path to socket(same folder as an agent by default).
`export AGENT_SOCKET_PATH=rn.socket`

After adding all robots, press key `p` or click on 'publish config' in footer

## Start Agent on robot
To start agent use:

`./robot-agent --owner <OWNER_PUBLIC_KEY> --secret-key <ROBOT_SECRET_KEY>`


| Argument               | Description                            | Default   |
| ---------------------- | -------------------------------------- | --------- |
| --socket-filename (-s) | path to create unix socket             | rn.socket |
| --key-filename (-k)    | path to save secret key                | rn.key    |
| --port-libp2p (-l)     | port to use as libp2p node             | 8765      |
| --bootstrap-addr (-b)  | multiaddress of bootstrap node         |           |
| --secret-key (-s)      | secret key in base64 to use on startup |           |
| --owner (-0)           | base64 public key of the owner         |           |
## See robots list
To see robots in network we use cli command
`rn robots list`

```
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━┳━━━━━━━━━┓
┃ PeerId                                               ┃ Name   ┃ Status  ┃
┡━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━╇━━━━━━━━━┩
│ 12D3KooWFGndT5BRXBSUGcQzT5zgYgyUVR6rbsVYTf4iSVA5Udob │ laptop │ Unknown │
│ 12D3KooWAgJuo1havrarkR4oy1zauEBTv9Bvg21g1V5qihhMnmEw │ robot  │ Unknown │
└──────────────────────────────────────────────────────┴────────┴─────────┘
```

## Start docker job
We can use CLI to launch docker job. `rn jobs add` command accepts path to json with job description and robot name(or peer_id) as arguments.

`rn jobs add <PATH_TO_JSON> <ROBOT>`

Let's start example `alpine:3` container (https://github.com/Smehnov/rn/blob/main/job_terminal.json):

`rn jobs add job_terminal.json turtlebot-0`

After sending command you will see a message with `job_id`
```
Preparing job:  521652a7-5715-496f-961d-1a0f1efbf1cc
Requests sent
```
## See jobs list
`rn jobs list <ROBOT>`

```
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━┓
┃ Job Id                               ┃ Job Type                ┃ Status     ┃
┡━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━━━━━┩
│ 521652a7-5715-496f-961d-1a0f1efbf1cc │ docker-container-launch │ processing │
└──────────────────────────────────────┴─────────────────────────┴────────────┘

```

## Connect to job's terminal

We can access job terminal if job was launched with `"custom_command": "sh"`

`rn jobs terminal <ROBOT> <JOB_ID>`
`rn jobs terminal turtlebot-0 521652a7-5715-496f-961d-1a0f1efbf1cc`

```
receiver started
===TERMINAL SESSION STARTED===

/ #
/ # ls
/ # bin    etc    lib    mnt    proc   run    srv    tmp    var
dev    home   media  opt    root   sbin   sys    usr
```

Use `Ctrl+D` to exit





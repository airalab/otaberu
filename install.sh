api_key=$1
echo $api_key
os=$(uname)
architecture=$(uname -m)

platform="$architecture-unknown-linux-musl"
if [[ "$os" == "Darwin" ]]; then
  platform="$architecture-apple-darwin"
fi

username=$(whoami)
agent_download_url="$(curl -s https://api.github.com/repos/merklebot/robot-agent/releases/latest | grep /robot-agent-$platform | cut -d : -f 2,3 | tr -d \")"



echo "platform: $platform"
echo "download url: $agent_download_url"


curl -o robot-agent -L $agent_download_url

chmod +x robot-agent

agent_binary_location=$(realpath robot-agent)

echo "agent location: $agent_binary_location"


SERVICE_NAME=robot-agent.service


echo "Creating systemd service"

service_path=/etc/systemd/system/merklebot.service

sudo tee -a $service_path << EOF
[Unit]
Description=Merklebot robot agent
After=network.target

[Service]
ExecStart=$agent_binary_location -a $api_key
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF


sudo systemctl enable merklebot
sudo systemctl start merklebot
echo "Service Started"

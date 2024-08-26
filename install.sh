owner_public_key=$1
secret_key=$2
echo $api_key
os=$(uname)
architecture=$(uname -m)

platform="$architecture-unknown-linux-musl"
if [[ "$os" == "Darwin" ]]; then
  platform="$architecture-apple-darwin"
fi

username=$(whoami)
agent_download_url="$(curl -s https://api.github.com/repos/otaberu/robot-agent/releases/latest | grep /robot-agent-$platform | cut -d : -f 2,3 | tr -d \")"



echo "platform: $platform"
echo "download url: $agent_download_url"


curl -o robot-agent -L $agent_download_url

chmod +x robot-agent

agent_binary_location=$(realpath robot-agent)

echo "agent location: $agent_binary_location"


SERVICE_NAME=robot-agent.service


echo "Creating systemd service"

service_path=/etc/systemd/system/robotagent.service

sudo tee -a $service_path << EOF
[Unit]
Description=Robot agent
After=network.target

[Service]
ExecStart=$agent_binary_location -o $owner_public_key -k $secret_key
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF


sudo systemctl enable robotagent 
sudo systemctl start robotagent
echo "Service Started"

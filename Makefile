deploy:
	cross build --target aarch64-unknown-linux-gnu --no-default-features --features backend-linuxkms-noseat --release
	ssh -t alarm@kodi.home "sudo systemctl stop raspberry-dashboard.service"
	scp target/aarch64-unknown-linux-gnu/release/raspberry-dashboard alarm@kodi.home:~/
	scp config.toml alarm@kodi.home:~/
	ssh -t alarm@kodi.home "sudo systemctl start raspberry-dashboard.service"

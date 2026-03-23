FROM rnestler/archlinuxarm-rust:1.94.0

RUN pacman -Sy --noconfirm fontconfig libxkbcommon libinput mesa

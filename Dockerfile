FROM rust:latest
WORKDIR /bot
COPY . .
RUN apt-get update --yes
RUN apt-get upgrade --yes
RUN apt-get install libopus0 cmake ffmpeg python python3-pip --yes
RUN python3 -m pip install -U yt-dlp
RUN cargo build --release
CMD DISCORD_TOKEN=$(cat TOKEN) cargo run --release
